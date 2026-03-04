import { useCallback, useMemo } from "react";
import { useParams } from "react-router-dom";
import { getAccount } from "../api/endpoints";
import type { AccountTx } from "../api/types";
import useTxDetails from "../hooks/useTxDetails";
import useInfiniteScroll from "../hooks/useInfiniteScroll";
import useAccountOverview from "../hooks/useAccountOverview";
import useAccountState from "../hooks/useAccountState";
import useAccountFilters from "../hooks/useAccountFilters";
import { FilteredTxTable } from "../components/TxRow";
import type { TxTableItem } from "../components/TxRow";
import InfiniteScrollSentinel from "../components/InfiniteScrollSentinel";
import AccountOverview from "../components/AccountOverview";
import AccountFilterBar from "../components/AccountFilterBar";

const BATCH_SIZE = 80;

export default function AccountDetail() {
  const { accountId } = useParams<{ accountId: string }>();
  const { filters, setFilters, filterKey, hasActiveFilters } = useAccountFilters();

  const fetchPage = useCallback(
    async (resumeToken?: string, limit?: number) => {
      const data = await getAccount(accountId!, {
        resume_token: resumeToken,
        limit: limit ?? BATCH_SIZE,
        ...filters,
      });
      return {
        items: data.account_txs,
        resumeToken: data.resume_token,
        totalCount: data.txs_count,
      };
    },
    [accountId, filters]
  );

  const {
    items: txns,
    totalCount,
    hasMore,
    loadMore,
    loading,
    loadingMore,
    error,
    aheadItems,
  } = useInfiniteScroll<AccountTx>({
    fetchPage,
    batchSize: BATCH_SIZE,
    key: `${accountId ?? ""}:${filterKey}`,
  });

  // All accumulated hashes + prefetched ahead buffer for preloading
  const hashes = useMemo(() => {
    const current = txns.map((t) => t.transaction_hash);
    const ahead = (aheadItems as AccountTx[]).map(
      (t) => t.transaction_hash
    );
    return [...current, ...ahead];
  }, [txns, aheadItems]);

  const { txMap } = useTxDetails(hashes, accountId);
  const { data: overview, loading: overviewLoading, error: overviewError } = useAccountOverview(accountId);
  const { data: accountState, loading: stateLoading } = useAccountState(accountId);

  const txItems: TxTableItem[] = useMemo(
    () =>
      txns.flatMap((atx) => {
        const parsed = txMap.get(atx.transaction_hash);
        return parsed
          ? [{ tx: parsed, timestamp: atx.tx_block_timestamp }]
          : [];
      }),
    [txns, txMap]
  );

  if (error) return <p className="text-red-600">{error}</p>;

  return (
    <div>
      <h1 className="mb-4 text-xl font-bold">
        Account: <span className="font-mono text-base">{accountId}</span>
      </h1>

      <AccountOverview data={overview} loading={overviewLoading} error={overviewError} accountState={accountState} stateLoading={stateLoading} />

      {totalCount > 0 && (
        <p className="mb-3 text-sm text-gray-600">
          Transactions ({totalCount.toLocaleString()})
        </p>
      )}

      <FilteredTxTable
        items={txItems}
        filterBar={(spam) => (
          <AccountFilterBar
            filters={filters}
            onChange={setFilters}
            hasActiveFilters={hasActiveFilters}
            spam={spam}
          />
        )}
      />

      {!loading && txItems.length === 0 && hasActiveFilters && (
        <p className="py-8 text-center text-sm text-gray-500">
          No transactions match the current filters.
        </p>
      )}

      <InfiniteScrollSentinel
        onLoadMore={loadMore}
        hasMore={hasMore}
        disabled={loading || txItems.length === 0}
        loadingMore={loadingMore}
      />
    </div>
  );
}
