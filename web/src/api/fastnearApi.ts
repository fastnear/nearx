import { networkId, nearxHeaders } from "../config";
import { fetchFastnearJson, getFastnearApiUrl } from "../tauri/runtime";
import {
  isRetryableHttpStatus,
  RetryableRequestError,
  retryAsync,
} from "./retry";

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

  const getHeaders = { "X-Nearx-Client": nearxHeaders["X-Nearx-Client"] };
  if (signal?.aborted) {
    throw new Error("request aborted");
  }

  let res = await fetchAccountFullJson(
    `${runtimeBaseUrl}/v1/account/${accountId}/full`,
    getHeaders,
  );

  // Runtime config may point to tx.* endpoints that don't expose /v1/account/*/full.
  // Fall back to the dedicated FastNEAR API host before surfacing failure.
  if (res.status === 404 && runtimeBaseUrl !== DEFAULT_BASE_URL && !signal?.aborted) {
    res = await fetchAccountFullJson(
      `${DEFAULT_BASE_URL}/v1/account/${accountId}/full`,
      getHeaders,
    );
  }

  if (res.status === 404) return null;
  if (!res.ok) {
    throw new Error(`API error ${res.status}: ${res.text ?? "request failed"}`);
  }
  if (res.body == null) {
    throw new Error(`API error ${res.status}: empty JSON response`);
  }
  return res.body;
}

async function fetchAccountFullJson(
  url: string,
  headers: Record<string, string>,
) {
  return retryAsync(async () => {
    const res = await fetchFastnearJson<AccountFullResponse | null>({
      url,
      method: "GET",
      headers,
      include_api_key: true,
    });
    if (isRetryableHttpStatus(res.status)) {
      throw new RetryableRequestError(
        res.text ?? `FastNEAR error ${res.status}`,
        res.status,
      );
    }
    return res;
  });
}
