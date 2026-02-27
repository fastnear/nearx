import { networkId } from "../config";
import { getFastnearApiUrl } from "../tauri/runtime";

const DEFAULT_BASE_URL =
  networkId === "testnet"
    ? "https://test.api.fastnear.com"
    : "https://api.fastnear.com";

export interface AccountFullState {
  balance: string;
  locked: string;
  storage_bytes: number;
}

export interface AccountFtToken {
  balance: string;
  contract_id: string;
  last_update_block_height: number | null;
}

export interface AccountNft {
  contract_id: string;
  last_update_block_height: number | null;
}

export interface AccountPool {
  pool_id: string;
  last_update_block_height: number | null;
}

export interface AccountFullResponse {
  account_id: string;
  state: AccountFullState | null;
  tokens: AccountFtToken[];
  nfts: AccountNft[];
  pools: AccountPool[];
}

export async function getAccountFull(
  accountId: string,
  signal?: AbortSignal,
): Promise<AccountFullResponse | null> {
  const runtimeBaseUrl = await getFastnearApiUrl(DEFAULT_BASE_URL);

  let res = await fetch(`${runtimeBaseUrl}/v1/account/${accountId}/full`, {
    signal,
  });

  // Runtime config may point to tx.* endpoints that don't expose /v1/account/*/full.
  // Fall back to the dedicated FastNEAR API host before surfacing failure.
  if (res.status === 404 && runtimeBaseUrl !== DEFAULT_BASE_URL) {
    res = await fetch(`${DEFAULT_BASE_URL}/v1/account/${accountId}/full`, {
      signal,
    });
  }

  if (res.status === 404) return null;
  if (!res.ok) {
    throw new Error(`API error ${res.status}: ${res.statusText}`);
  }
  return res.json();
}
