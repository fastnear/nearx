import { fetchApi } from "./client";
import type {
  BlocksResponse,
  BlockDetailResponse,
  TransactionsResponse,
  AccountResponse,
  AccountFilters,
} from "./types";

export function getBlocks(
  params: { limit?: number; desc?: boolean; to_block_height?: number; from_block_height?: number } = {}
): Promise<BlocksResponse> {
  return fetchApi<BlocksResponse>("blocks", params);
}

export function getBlock(
  blockId: string | number,
  opts: { with_transactions?: boolean } = {}
): Promise<BlockDetailResponse> {
  return fetchApi<BlockDetailResponse>("block", { block_id: blockId, ...opts });
}

export function getTransactions(
  txHashes: string[]
): Promise<TransactionsResponse> {
  return fetchApi<TransactionsResponse>("transactions", {
    tx_hashes: txHashes,
  });
}

export function getAccount(
  accountId: string,
  filters: { resume_token?: string; limit?: number } & AccountFilters = {}
): Promise<AccountResponse> {
  return fetchApi<AccountResponse>("account", {
    account_id: accountId,
    ...filters,
  });
}
