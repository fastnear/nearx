import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBlocks } from "../api/endpoints";
import type { BlockHeader } from "../api/types";
import useInfiniteScroll from "../hooks/useInfiniteScroll";
import useBlockFilters from "../hooks/useBlockFilters";
import BlockHeight from "../components/BlockHeight";
import AccountId from "../components/AccountId";
import InfiniteScrollSentinel from "../components/InfiniteScrollSentinel";
import TimeAgo from "../components/TimeAgo";
import BlockFilterBar from "../components/BlockFilterBar";

const BATCH_SIZE = 80;
const LIVE_POLL_INTERVAL_MS = 999;
const LIVE_POLL_LIMIT = 24;
const MAX_LIVE_HEAD_BLOCKS = 240;

function dedupeByHeight(blocks: BlockHeader[]): BlockHeader[] {
  const seen = new Set<number>();
  const deduped: BlockHeader[] = [];

  for (const block of blocks) {
    if (seen.has(block.block_height)) {
      continue;
    }

    seen.add(block.block_height);
    deduped.push(block);
  }

  return deduped;
}

export default function Home() {
  const { filters, setFilters, filterKey, hasActiveFilters } = useBlockFilters();
  const isDesc = filters.desc !== false;

  const fetchPage = useCallback(
    async (resumeToken?: string, limit?: number) => {
      const params: {
        limit: number;
        desc: boolean;
        to_block_height?: number;
        from_block_height?: number;
      } = {
        limit: limit ?? BATCH_SIZE,
        desc: isDesc,
      };

      if (isDesc) {
        params.to_block_height = resumeToken
          ? Number(resumeToken)
          : filters.to_block_height;
        params.from_block_height = filters.from_block_height;
      } else {
        params.from_block_height = resumeToken
          ? Number(resumeToken)
          : filters.from_block_height;
        params.to_block_height = filters.to_block_height;
      }

      const data = await getBlocks(params);
      const blocks = data.blocks;
      let nextToken: string | undefined;
      if (blocks.length > 0) {
        const lastBlock = blocks[blocks.length - 1];
        nextToken = isDesc
          ? String(lastBlock.block_height - 1)
          : String(lastBlock.block_height + 1);
      }

      return {
        items: blocks,
        resumeToken: nextToken,
        totalCount: 0,
      };
    },
    [filters, isDesc]
  );

  const {
    items: blocks,
    hasMore,
    loadMore,
    loading,
    loadingMore,
    error,
  } = useInfiniteScroll<BlockHeader>({
    fetchPage,
    batchSize: BATCH_SIZE,
    key: `blocks:${filterKey}`,
  });

  const [liveHeadBlocks, setLiveHeadBlocks] = useState<BlockHeader[]>([]);
  const liveUpdatesEnabled = isDesc && !hasActiveFilters;

  useEffect(() => {
    setLiveHeadBlocks([]);
  }, [filterKey]);

  const renderedBlocks = useMemo(
    () => dedupeByHeight([...liveHeadBlocks, ...blocks]),
    [blocks, liveHeadBlocks]
  );

  const topRenderedHeightRef = useRef<number | null>(null);
  useEffect(() => {
    topRenderedHeightRef.current = renderedBlocks[0]?.block_height ?? null;
  }, [renderedBlocks]);

  const pollForHeadUpdates = useCallback(async (opts?: { force?: boolean }) => {
    if (!liveUpdatesEnabled) {
      return;
    }
    if (!opts?.force && document.visibilityState !== "visible") {
      return;
    }

    const currentTop = topRenderedHeightRef.current;

    try {
      const data = await getBlocks({
        limit: LIVE_POLL_LIMIT,
        desc: true,
      }, { bypassCache: true });

      const incoming =
        currentTop == null
          ? data.blocks
          : data.blocks.filter((block) => block.block_height > currentTop);

      if (incoming.length === 0) {
        return;
      }

      setLiveHeadBlocks((prev) =>
        dedupeByHeight([...incoming, ...prev]).slice(0, MAX_LIVE_HEAD_BLOCKS)
      );
    } catch {
      // Ignore live poll errors to avoid interrupting normal list usage.
    }
  }, [liveUpdatesEnabled]);

  useEffect(() => {
    if (!liveUpdatesEnabled) {
      return;
    }

    void pollForHeadUpdates({ force: true });

    const intervalId = window.setInterval(() => {
      void pollForHeadUpdates();
    }, LIVE_POLL_INTERVAL_MS);

    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        void pollForHeadUpdates({ force: true });
      }
    };

    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      window.clearInterval(intervalId);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [liveUpdatesEnabled, pollForHeadUpdates]);

  if (error) {
    return <p className="text-red-600">Error loading blocks: {error}</p>;
  }

  return (
    <div>
      <h1 className="mb-4 text-xl font-bold">Latest Blocks</h1>
      {/* Desktop table */}
      <div className="hidden sm:block overflow-x-auto rounded-lg border border-gray-200 bg-surface">
        <BlockFilterBar
          filters={filters}
          onChange={setFilters}
          hasActiveFilters={hasActiveFilters}
        />
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-gray-200 bg-gray-50 text-left text-xs font-medium uppercase text-gray-500">
              <th className="px-4 py-3">Height</th>
              <th className="px-4 py-3">Time</th>
              <th className="px-4 py-3">Author</th>
              <th className="px-4 py-3 text-right">Txns</th>
              <th className="px-4 py-3 text-right">Receipts</th>
              <th className="px-4 py-3 text-right">Gas Used</th>
            </tr>
          </thead>
          <tbody>
            {renderedBlocks.map((b) => (
              <tr
                key={b.block_height}
                className="border-b border-gray-100 hover:bg-gray-50"
              >
                <td className="px-4 py-3">
                  <span className="font-medium">
                    <BlockHeight height={b.block_height} />
                  </span>
                </td>
                <td className="px-4 py-3 text-gray-500">
                  <TimeAgo timestampNs={b.block_timestamp} />
                </td>
                <td className="px-4 py-3">
                  <AccountId accountId={b.author_id} />
                </td>
                <td className="px-4 py-3 text-right">{b.num_transactions}</td>
                <td className="px-4 py-3 text-right">{b.num_receipts}</td>
                <td className="px-4 py-3 text-right">
                  {(Number(b.gas_burnt) / 1e12).toFixed(2)} Tgas
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {/* Mobile cards */}
      <div className="sm:hidden rounded-lg border border-gray-200 bg-surface divide-y divide-gray-100">
        <BlockFilterBar
          filters={filters}
          onChange={setFilters}
          hasActiveFilters={hasActiveFilters}
        />
        {renderedBlocks.map((b) => (
          <div key={b.block_height} className="px-3 py-2.5">
            <div className="flex items-center justify-between gap-2 mb-0.5">
              <span className="font-medium text-sm">
                <BlockHeight height={b.block_height} />
              </span>
              <span className="text-xs text-gray-500 shrink-0">
                <TimeAgo timestampNs={b.block_timestamp} />
              </span>
            </div>
            <div className="flex items-center justify-between gap-2">
              <AccountId accountId={b.author_id} />
              <span className="text-xs text-gray-500 shrink-0">{b.num_transactions} txns</span>
            </div>
          </div>
        ))}
      </div>
      {!loading && renderedBlocks.length === 0 && hasActiveFilters && (
        <p className="py-8 text-center text-sm text-gray-500">
          No blocks match the current filters.
        </p>
      )}
      <InfiniteScrollSentinel
        onLoadMore={loadMore}
        hasMore={hasMore}
        disabled={loading}
        loadingMore={loadingMore}
      />
    </div>
  );
}
