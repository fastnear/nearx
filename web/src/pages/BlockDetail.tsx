import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { getBlock, getBlocks } from "../api/endpoints";
import type { BlockHeader, BlockTx } from "../api/types";
import BlockHash from "../components/BlockHash";
import BlockHeight from "../components/BlockHeight";
import AccountId from "../components/AccountId";
import TimeAgo from "../components/TimeAgo";
import InfiniteScrollSentinel from "../components/InfiniteScrollSentinel";
import useTxDetails from "../hooks/useTxDetails";
import { FilteredTxTable } from "../components/TxRow";
import type { TxTableItem } from "../components/TxRow";
import GasAmount from "../components/GasAmount";
import NearAmount from "../components/NearAmount";

const VISIBLE_STEP = 40;

export default function BlockDetail() {
  const { blockId } = useParams<{ blockId: string }>();
  const [block, setBlock] = useState<BlockHeader | null>(null);
  const [txs, setTxs] = useState<BlockTx[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [visibleCount, setVisibleCount] = useState(VISIBLE_STEP);
  const [nextBlockHeight, setNextBlockHeight] = useState<number | null>(null);

  useEffect(() => {
    if (!blockId) return;
    setVisibleCount(VISIBLE_STEP);
    setNextBlockHeight(null);
    getBlock(/^\d+$/.test(blockId) ? Number(blockId) : blockId, { with_transactions: true })
      .then((data) => {
        if (!data.block) {
          setError("Block not found");
          return;
        }
        setBlock(data.block);
        setTxs(data.block_txs || []);
        // Fire-and-forget: fetch next block without blocking UI
        getBlocks({
          from_block_height: data.block.block_height + 1,
          desc: false,
          limit: 1,
        })
          .then((res) => {
            if (res.blocks.length > 0) {
              setNextBlockHeight(res.blocks[0].block_height);
            }
          })
          .catch(() => {});
      })
      .catch((err) => setError(String(err)));
  }, [blockId]);

  const visibleTxs = useMemo(
    () => txs.slice(0, visibleCount),
    [txs, visibleCount]
  );
  const hasMore = visibleCount < txs.length;

  const loadMore = useCallback(
    () => setVisibleCount((c) => Math.min(c + VISIBLE_STEP, txs.length)),
    [txs.length]
  );

  // Fetch details for visible txs + one step ahead for preloading
  const visibleHashes = useMemo(
    () => txs.slice(0, visibleCount + VISIBLE_STEP).map((t) => t.transaction_hash),
    [txs, visibleCount]
  );
  const { txMap } = useTxDetails(visibleHashes);

  const txItems: TxTableItem[] = useMemo(
    () =>
      visibleTxs.flatMap((btx) => {
        const parsed = txMap.get(btx.transaction_hash);
        return parsed
          ? [{ tx: parsed, timestamp: btx.tx_block_timestamp }]
          : [];
      }),
    [visibleTxs, txMap]
  );

  if (error) return <p className="text-red-600">{error}</p>;
  if (!block) return <p className="text-gray-500">Loading block...</p>;

  return (
    <div>
      <h1 className="mb-4 text-xl font-bold">
        Block #<Link to={`/block/${block.block_height}`} className="hover:underline">{block.block_height.toLocaleString()}</Link>
      </h1>

      <div className="mb-6 rounded-lg border border-gray-200 bg-surface text-sm">
        <dl className="grid gap-px sm:grid-cols-2 [&>div]:flex [&>div]:min-w-0 [&>div]:gap-2 [&>div]:border-b [&>div]:border-gray-100 [&>div]:px-4 [&>div]:py-3 [&>div:last-child]:border-b-0 [&>div:nth-last-child(2)]:sm:border-b-0">
          <div>
            <dt className="shrink-0 text-gray-500">Hash</dt>
            <dd className="min-w-0 truncate"><BlockHash hash={block.block_hash} /></dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Timestamp</dt>
            <dd><TimeAgo timestampNs={block.block_timestamp} /></dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Author</dt>
            <dd><AccountId accountId={block.author_id} /></dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Epoch ID</dt>
            <dd className="min-w-0 truncate font-mono text-xs">{block.epoch_id}</dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Prev Block</dt>
            <dd>{block.prev_block_height != null ? <BlockHeight height={block.prev_block_height} /> : <BlockHash hash={block.prev_block_hash} />}</dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Next Block</dt>
            <dd>{nextBlockHeight ? <BlockHeight height={nextBlockHeight} /> : <span className="text-gray-400">—</span>}</dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Transactions</dt>
            <dd>{block.num_transactions}</dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Receipts</dt>
            <dd>{block.num_receipts}</dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Gas Used</dt>
            <dd><GasAmount gas={Number(block.gas_burnt)} /></dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Gas Price</dt>
            <dd><NearAmount yoctoNear={block.gas_price} /></dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Tokens Burnt</dt>
            <dd><NearAmount yoctoNear={block.tokens_burnt} showPrice /></dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Chunks</dt>
            <dd>{block.chunks_included}</dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Protocol Version</dt>
            <dd>{block.protocol_version}</dd>
          </div>
        </dl>
      </div>

      {txs.length > 0 && (
        <>
          <p className="mb-3 text-sm text-gray-600">
            Transactions ({block.num_transactions.toLocaleString()})
          </p>
          <FilteredTxTable items={txItems} />
          <InfiniteScrollSentinel
            onLoadMore={loadMore}
            hasMore={hasMore}
            disabled={txItems.length === 0}
          />
        </>
      )}
    </div>
  );
}
