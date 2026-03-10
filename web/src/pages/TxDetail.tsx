import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useParams, useLocation, Link } from "react-router-dom";
import { getTransactions } from "../api/endpoints";
import type { TransactionDetail } from "../api/types";
import { getMatchingWidgets } from "../widgets/registry";
import { parseTransaction } from "../utils/parseTransaction";
import GasAmount from "../components/GasAmount";
import NearAmount from "../components/NearAmount";
import TransactionHash from "../components/TransactionHash";
import AccountId from "../components/AccountId";
import BlockHeight from "../components/BlockHeight";
import TimeAgo from "../components/TimeAgo";
import TransferSummary, { NftTransferSummary } from "../components/TransferSummary";
import ReceiptCard from "../components/ReceiptCard";
import { CircleCheck, CircleX, Clock, Radio } from "lucide-react";

const PENDING_TX_REFRESH_INTERVAL_MS = 999;
const NOT_FOUND_RETRY_INTERVAL_MS = 2_000;
const NOT_FOUND_MAX_RETRIES = 7;

export default function TxDetail() {
  const { txHash } = useParams<{ txHash: string }>();
  const { hash } = useLocation();
  const [tx, setTx] = useState<TransactionDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const scrolledRef = useRef(false);
  const notFoundRetriesRef = useRef(0);
  const notFoundTimerRef = useRef<number | null>(null);

  const fetchTx = useCallback(
    async (opts?: { bypassCache?: boolean }) => {
      if (!txHash) {
        return;
      }

      try {
        const data = await getTransactions([txHash], {
          bypassCache: opts?.bypassCache ?? false,
        });

        if (data.transactions.length > 0) {
          setTx(data.transactions[0]);
          setError(null);
          notFoundRetriesRef.current = 0;
        } else if (notFoundRetriesRef.current < NOT_FOUND_MAX_RETRIES) {
          // Tx may not be indexed yet (e.g. just broadcast). Retry with delay.
          notFoundRetriesRef.current++;
          console.log(
            `[tx-detail] tx ${txHash} not found, retry ${notFoundRetriesRef.current}/${NOT_FOUND_MAX_RETRIES}`,
          );
          notFoundTimerRef.current = window.setTimeout(() => {
            void fetchTx({ bypassCache: true });
          }, NOT_FOUND_RETRY_INTERVAL_MS);
        } else {
          setError("Transaction not found");
        }
      } catch (err) {
        setError(String(err));
      }
    },
    [txHash],
  );

  useEffect(() => {
    if (!txHash) return;
    scrolledRef.current = false;
    notFoundRetriesRef.current = 0;
    if (notFoundTimerRef.current !== null) {
      window.clearTimeout(notFoundTimerRef.current);
      notFoundTimerRef.current = null;
    }
    void fetchTx({ bypassCache: true });
    return () => {
      if (notFoundTimerRef.current !== null) {
        window.clearTimeout(notFoundTimerRef.current);
        notFoundTimerRef.current = null;
      }
    };
  }, [txHash, fetchTx]);

  const parsed = useMemo(() => (tx ? parseTransaction(tx) : null), [tx]);

  useEffect(() => {
    if (!txHash || !parsed || parsed.is_success !== null) {
      return;
    }

    const poll = () => {
      if (document.visibilityState !== "visible") {
        return;
      }
      void fetchTx({ bypassCache: true });
    };

    const intervalId = window.setInterval(poll, PENDING_TX_REFRESH_INTERVAL_MS);

    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        void fetchTx({ bypassCache: true });
      }
    };

    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      window.clearInterval(intervalId);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [txHash, parsed?.is_success, fetchTx]);

  // Scroll to anchor after tx loads (elements don't exist during initial load)
  useEffect(() => {
    if (!tx || !hash || scrolledRef.current) return;
    scrolledRef.current = true;
    requestAnimationFrame(() => {
      const el = document.getElementById(hash.slice(1));
      if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }, [tx, hash]);

  if (error) return <p className="text-red-600">{error}</p>;
  if (!tx) return <p className="text-gray-500">Loading transaction...</p>;

  const parsedTx = parsed;
  if (!parsedTx) return <p className="text-gray-500">Loading transaction...</p>;

  const allWidgets = getMatchingWidgets(tx);
  const explanationWidgets = allWidgets.filter((w) => w.type === "explanation");
  const utilityWidgets = allWidgets.filter((w) => w.type === "utility");

  // Total gas and tokens burnt across all outcomes
  let totalGas = tx.execution_outcome.outcome.gas_burnt;
  let totalTokensBurnt = BigInt(tx.execution_outcome.outcome.tokens_burnt);
  for (const r of tx.receipts) {
    totalGas += r.execution_outcome.outcome.gas_burnt;
    totalTokensBurnt += BigInt(r.execution_outcome.outcome.tokens_burnt);
  }

  return (
    <div>
      <h1 className="mb-4 text-xl font-bold">Transaction</h1>

      <div className="mb-6 rounded-lg border border-gray-200 bg-surface text-sm">
        <dl className="grid gap-px sm:grid-cols-2 [&>div]:flex [&>div]:items-center [&>div]:gap-2 [&>div]:border-b [&>div]:border-gray-100 [&>div]:px-4 [&>div]:py-2 [&>div:last-child]:border-b-0 [&>div:nth-last-child(2)]:sm:border-b-0">
          <div className="sm:col-span-2">
            <dt className="shrink-0 text-gray-500">Hash</dt>
            <dd className="flex flex-1 min-w-0 items-center justify-between gap-2">
              <span className="break-all">
                <TransactionHash hash={tx.transaction.hash} truncate={false} />
              </span>
              <span className="flex shrink-0 items-center gap-1">
                {parsedTx.is_success === null ? (
                  <Clock className="size-3.5 text-yellow-500" />
                ) : parsedTx.is_success ? (
                  <CircleCheck className="size-3.5 text-green-600" />
                ) : (
                  <CircleX className="size-3.5 text-red-600" />
                )}
              </span>
            </dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Signer</dt>
            <dd className="flex items-center gap-1">
              {parsedTx.relayer_id && (
                <Link
                  to={`/account/${parsedTx.relayer_id}`}
                  title={`Relayed by ${parsedTx.relayer_id}`}
                >
                  <Radio className="size-3.5 text-red-500" />
                </Link>
              )}
              <AccountId accountId={parsedTx.signer_id} />
            </dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Receiver</dt>
            <dd>
              <AccountId accountId={parsedTx.receiver_id} />
            </dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Block</dt>
            <dd>
              <BlockHeight height={tx.execution_outcome.block_height} />
            </dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Time</dt>
            <dd>
              <TimeAgo
                timestampNs={String(tx.execution_outcome.block_timestamp)}
              />
            </dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Gas Used</dt>
            <dd><GasAmount gas={totalGas} /></dd>
          </div>
          <div>
            <dt className="shrink-0 text-gray-500">Tokens Burnt</dt>
            <dd><NearAmount yoctoNear={totalTokensBurnt.toString()} /></dd>
          </div>
        </dl>
      </div>

      {(parsedTx.transfers.length > 0 || parsedTx.nftTransfers.length > 0) && (
        <div className="mb-6 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="border-b border-gray-100 px-4 py-2">
            <h2 className="text-xs font-medium uppercase text-gray-500">Transfers</h2>
          </div>
          <div className="flex flex-col gap-1 px-4 py-3">
            {parsedTx.transfers.map((t, i) => (
              <TransferSummary key={`ft-${i}`} transfer={t} />
            ))}
            {parsedTx.nftTransfers.map((t, i) => (
              <NftTransferSummary key={`nft-${i}`} transfer={t} />
            ))}
          </div>
        </div>
      )}

      {explanationWidgets.map((w) => (
        <w.component key={w.id} tx={tx} />
      ))}

      {tx.receipts.length > 0 && (
        <div className="mb-6">
          <h2 className="mb-3 text-lg font-semibold">
            Receipts ({tx.receipts.length})
          </h2>
          <div className="space-y-3">
            {tx.receipts.map((r) => (
              <ReceiptCard key={r.receipt.receipt_id} r={r} />
            ))}
          </div>
        </div>
      )}

      {utilityWidgets.map((w) => (
        <w.component key={w.id} tx={tx} />
      ))}
    </div>
  );
}
