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

export async function broadcastTransaction(
  signedTxBase64: string,
): Promise<unknown> {
  const rpcUrl = await getNearNodeUrl(DEFAULT_RPC_URL);
  const res = await fetch(rpcUrl, {
    method: "POST",
    headers: nearxHeaders,
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "1",
      method: "broadcast_tx_commit",
      params: [signedTxBase64],
    }),
  });
  const json = await res.json();
  if (json.error) {
    throw new Error(json.error.message ?? JSON.stringify(json.error));
  }
  return json.result;
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
