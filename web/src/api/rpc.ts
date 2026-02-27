import { networkId } from "../config";
import { getNearNodeUrl } from "../tauri/runtime";

const DEFAULT_RPC_URL =
  networkId === "testnet"
    ? "https://rpc.testnet.fastnear.com"
    : "https://rpc.mainnet.fastnear.com";

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
  const rpcUrl = await getNearNodeUrl(DEFAULT_RPC_URL);
  const res = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "1",
      method: "query",
      params: {
        request_type: "view_account",
        finality: "final",
        account_id: accountId,
      },
    }),
    signal,
  });
  const json = await res.json();
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

export async function viewCall<T>(
  contractId: string,
  methodName: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const rpcUrl = await getNearNodeUrl(DEFAULT_RPC_URL);
  const argsBase64 = btoa(JSON.stringify(args));
  const res = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
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
    }),
  });
  const json = await res.json();
  if (json.error) {
    throw new Error(json.error.message ?? JSON.stringify(json.error));
  }
  const bytes = new Uint8Array(json.result.result);
  const text = new TextDecoder().decode(bytes);
  return JSON.parse(text) as T;
}
