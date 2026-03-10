import { networkId, nearxHeaders } from "../config";
import { getNearNodeUrl } from "../tauri/runtime";
import {
  isRetryableHttpStatus,
  isRetryableMessage,
  linkAbortSignals,
  RequestTimeoutError,
  RetryableRequestError,
  retryAsync,
} from "./retry";

const DEFAULT_RPC_URL =
  networkId === "testnet"
    ? "https://rpc.testnet.fastnear.com"
    : "https://rpc.mainnet.fastnear.com";
const DEFAULT_RPC_TIMEOUT_MS = 8_000;

const NO_CONTRACT_CODE_HASH = "11111111111111111111111111111111";

export interface AccountState {
  amount: string;
  locked: string;
  code_hash: string;
  storage_usage: number;
  block_height: number;
  block_hash: string;
  hasContract: boolean;
}

export async function viewAccount(
  accountId: string,
  signal?: AbortSignal,
): Promise<AccountState | null> {
  const json = await rpcJsonRequest(
    {
      jsonrpc: "2.0",
      id: "1",
      method: "query",
      params: {
        request_type: "view_account",
        finality: "final",
        account_id: accountId,
      },
    },
    { signal },
  );
  if (json.error) {
    if (json.error.cause?.name === "UNKNOWN_ACCOUNT") return null;
    throw new Error(json.error.message ?? JSON.stringify(json.error));
  }
  const r = json.result;
  return {
    amount: r.amount,
    locked: r.locked,
    code_hash: r.code_hash,
    storage_usage: r.storage_usage,
    block_height: r.block_height,
    block_hash: r.block_hash,
    hasContract: r.code_hash !== NO_CONTRACT_CODE_HASH,
  };
}

export interface AccessKeyView {
  nonce: number;
  block_hash: string;
  permission: unknown;
}

export async function viewAccessKey(
  accountId: string,
  publicKey: string,
  signal?: AbortSignal,
): Promise<AccessKeyView> {
  const json = await rpcJsonRequest(
    {
      jsonrpc: "2.0",
      id: "1",
      method: "query",
      params: {
        request_type: "view_access_key",
        finality: "final",
        account_id: accountId,
        public_key: publicKey,
      },
    },
    { signal },
  );
  if (json.error) {
    throw new Error(json.error.message ?? JSON.stringify(json.error));
  }
  const r = json.result;
  return {
    nonce: r.nonce,
    block_hash: r.block_hash,
    permission: r.permission,
  };
}

const BROADCAST_COMMIT_TIMEOUT_MS = 30_000;
const BROADCAST_ASYNC_TIMEOUT_MS = 10_000;

// NEAR RPC errors where the transaction itself is invalid — async fallback
// won't help because validators will also reject it.
function isInvalidTransactionError(error: Record<string, unknown>): boolean {
  const cause = error.cause as Record<string, unknown> | undefined;
  const causeName = typeof cause?.name === "string" ? cause.name : "";
  return causeName === "INVALID_TRANSACTION";
}

/** Extract a descriptive error string from NEAR RPC error JSON.
 *  The top-level `message` is usually just "Server error" — the real detail
 *  lives in `cause.name`, `cause.info`, and `data`. */
function describeRpcError(error: Record<string, unknown>): string {
  const parts: string[] = [];
  const topMessage = typeof error.message === "string" ? error.message : "";
  const cause = error.cause as Record<string, unknown> | undefined;
  const causeName = typeof cause?.name === "string" ? cause.name : "";
  const causeInfo = cause?.info;

  if (causeName) {
    parts.push(causeName);
  } else if (topMessage) {
    parts.push(topMessage);
  }

  if (causeInfo && typeof causeInfo === "object") {
    // causeInfo often has an `error_message` string with the real explanation
    const infoRec = causeInfo as Record<string, unknown>;
    if (typeof infoRec.error_message === "string") {
      parts.push(infoRec.error_message);
    } else {
      parts.push(JSON.stringify(causeInfo));
    }
  }

  const data = error.data;
  if (typeof data === "string" && data.trim()) {
    parts.push(data);
  } else if (data && typeof data === "object") {
    parts.push(JSON.stringify(data));
  }

  return parts.join(": ") || topMessage || JSON.stringify(error);
}

export async function broadcastTransaction(
  signedTxBase64: string,
): Promise<unknown> {
  const rpcUrl = await getNearNodeUrl(DEFAULT_RPC_URL);

  // Try broadcast_tx_commit first — returns full execution result.
  // 30s timeout: this RPC method blocks until the tx is included in a block
  // (~1-2s normally, but can exceed 10s under load).
  let commitError: string | undefined;
  let commitJson: Record<string, unknown> | undefined;
  console.log("[broadcast] starting broadcast_tx_commit to", rpcUrl, "payload_len=", signedTxBase64.length);
  try {
    const controller = new AbortController();
    const timeout = window.setTimeout(
      () => controller.abort(),
      BROADCAST_COMMIT_TIMEOUT_MS,
    );
    try {
      const res = await fetch(rpcUrl, {
        method: "POST",
        headers: nearxHeaders,
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: "1",
          method: "broadcast_tx_commit",
          params: [signedTxBase64],
        }),
        signal: controller.signal,
      });
      console.log("[broadcast] commit HTTP status:", res.status);
      commitJson = await res.json();
      if (!commitJson!.error) {
        console.log("[broadcast] commit success", (commitJson!.result as Record<string, unknown>)?.transaction);
        return commitJson!.result;
      }
      const rpcError = commitJson!.error as Record<string, unknown>;
      commitError = describeRpcError(rpcError);
      console.warn("[broadcast] commit RPC error:", commitError);
      console.warn("[broadcast] full error object:", JSON.stringify(rpcError, null, 2));

      // If the transaction itself is invalid (bad nonce, bad signature, expired
      // block_hash, etc.), async fallback will NOT help — throw immediately.
      if (isInvalidTransactionError(rpcError)) {
        console.error("[broadcast] transaction is invalid, skipping async fallback");
        throw new Error(commitError);
      }
    } finally {
      window.clearTimeout(timeout);
    }
  } catch (err) {
    // Re-throw invalid-transaction errors without masking them.
    if (commitError && commitJson?.error && isInvalidTransactionError(commitJson.error as Record<string, unknown>)) {
      throw err;
    }
    // Timeout or network error — fall through to async fallback.
    commitError = commitError ?? "broadcast_tx_commit timed out";
    console.warn("[broadcast] commit fetch error:", err instanceof Error ? err.message : err);
  }

  // broadcast_tx_commit returned a server/timeout error. Fall back to
  // broadcast_tx_async which returns immediately after mempool acceptance.
  console.log("[broadcast] falling back to broadcast_tx_async, commit error was:", commitError);
  const controller = new AbortController();
  const timeout = window.setTimeout(
    () => controller.abort(),
    BROADCAST_ASYNC_TIMEOUT_MS,
  );
  try {
    const asyncRes = await fetch(rpcUrl, {
      method: "POST",
      headers: nearxHeaders,
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "2",
        method: "broadcast_tx_async",
        params: [signedTxBase64],
      }),
      signal: controller.signal,
    });
    console.log("[broadcast] async HTTP status:", asyncRes.status);
    const asyncJson = await asyncRes.json();
    if (asyncJson.error) {
      const asyncError = describeRpcError(asyncJson.error as Record<string, unknown>);
      console.error("[broadcast] async RPC error:", asyncError);
      console.error("[broadcast] async full error:", JSON.stringify(asyncJson.error, null, 2));
      throw new Error(commitError);
    }
    console.log("[broadcast] async accepted, tx hash:", asyncJson.result);
  } catch (asyncErr) {
    // Async fallback failed (network error, timeout, or RPC error).
    // Throw the original commit error since it has more context.
    console.error("[broadcast] async fallback failed:", asyncErr instanceof Error ? asyncErr.message : asyncErr);
    throw asyncErr instanceof Error && asyncErr.message === commitError
      ? asyncErr
      : new Error(commitError);
  } finally {
    window.clearTimeout(timeout);
  }

  // Async submission confirmed — return a marker so callers know the tx was
  // submitted but execution status is unknown.
  console.log("[broadcast] returning async_submitted marker");
  return { async_submitted: true };
}

export async function viewCall<T>(
  contractId: string,
  methodName: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const argsBase64 = btoa(JSON.stringify(args));
  const json = await rpcJsonRequest({
    jsonrpc: "2.0",
    id: "1",
    method: "query",
    params: {
      request_type: "call_function",
      finality: "final",
      account_id: contractId,
      method_name: methodName,
      args_base64: argsBase64,
    },
  });
  if (json.error) {
    throw new Error(json.error.message ?? JSON.stringify(json.error));
  }
  const bytes = new Uint8Array(json.result.result);
  const text = new TextDecoder().decode(bytes);
  return JSON.parse(text) as T;
}

interface RpcRequestOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
}

async function rpcJsonRequest(
  body: Record<string, unknown>,
  options: RpcRequestOptions = {},
): Promise<any> {
  const rpcUrl = await getNearNodeUrl(DEFAULT_RPC_URL);
  const timeoutMs = options.timeoutMs ?? DEFAULT_RPC_TIMEOUT_MS;
  return retryAsync(async () => {
    const controller = new AbortController();
    const unlink = linkAbortSignals(options.signal, controller);
    const timeout = window.setTimeout(
      () => controller.abort(new RequestTimeoutError(timeoutMs)),
      timeoutMs,
    );
    try {
      const res = await fetch(rpcUrl, {
        method: "POST",
        headers: nearxHeaders,
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      const text = await res.text();
      const parseJson = () => (text.trim() ? JSON.parse(text) : {});

      if (isRetryableHttpStatus(res.status)) {
        throw new RetryableRequestError(
          text || `RPC error ${res.status}`,
          res.status,
        );
      }
      const json = parseJson();
      if (!res.ok) {
        throw new Error(json?.error?.message ?? `RPC error ${res.status}`);
      }
      if (json?.error) {
        const message = json.error.message ?? JSON.stringify(json.error);
        if (isRetryableMessage(message)) {
          throw new RetryableRequestError(message);
        }
        throw new Error(message);
      }
      return json;
    } catch (error) {
      if (options.signal?.aborted) {
        throw error;
      }
      if (
        error instanceof DOMException &&
        error.name === "AbortError" &&
        !options.signal?.aborted
      ) {
        throw new RequestTimeoutError(timeoutMs);
      }
      throw error;
    } finally {
      window.clearTimeout(timeout);
      unlink();
    }
  });
}
