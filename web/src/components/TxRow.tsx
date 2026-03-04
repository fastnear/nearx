import { useMemo, useState, useCallback } from "react";
import type { ParsedTx } from "../utils/parseTransaction";
import type { ParsedAction } from "../utils/parseTransaction";
import GasAmount from "./GasAmount";
import TransactionHash from "./TransactionHash";
import TimeAgo from "./TimeAgo";
import AccountId from "./AccountId";
import Action from "./Action";
import ReceiptPopup from "./ReceiptPopup";
import TransferSummary, { NftTransferSummary } from "./TransferSummary";
import type { TransferInfo, NftTransferInfo } from "../utils/parseTransaction";
import { CircleCheck, CircleX, Clock, SlidersHorizontal, Radio } from "lucide-react";
import { Link } from "react-router-dom";
import useSpamTokens, { isSpam } from "../hooks/useSpamTokens";
import useSpamNfts, { isSpamNft } from "../hooks/useSpamNfts";

const ACTIONS_LIMIT = 3;
const TRANSFERS_LIMIT = 3;

function ActionList({
  actions,
  onActionClick,
}: {
  actions: ParsedAction[];
  onActionClick?: (index: number) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const showAll = expanded || actions.length <= ACTIONS_LIMIT;
  const visible = showAll ? actions : actions.slice(0, ACTIONS_LIMIT);

  return (
    <>
      {visible.map((a, i) => (
        <div
          key={i}
          onClick={onActionClick ? (e) => { e.stopPropagation(); onActionClick(i); } : undefined}
          className={onActionClick ? "cursor-pointer hover:text-blue-600" : undefined}
        >
          <Action action={a} />
        </div>
      ))}
      {!showAll && (
        <button
          onClick={() => setExpanded(true)}
          className="cursor-pointer text-blue-600 hover:text-blue-800"
        >
          + {actions.length - ACTIONS_LIMIT} more...
        </button>
      )}
    </>
  );
}

function TransferList({
  transfers,
  nftTransfers,
}: {
  transfers: TransferInfo[];
  nftTransfers: NftTransferInfo[];
}) {
  const [expanded, setExpanded] = useState(false);
  const totalCount = transfers.length + nftTransfers.length;
  const showAll = expanded || totalCount <= TRANSFERS_LIMIT;

  if (showAll) {
    return (
      <>
        {transfers.map((t, i) => (
          <TransferSummary key={`ft-${i}`} transfer={t} />
        ))}
        {nftTransfers.map((t, i) => (
          <NftTransferSummary key={`nft-${i}`} transfer={t} />
        ))}
      </>
    );
  }

  // Show limited items: FT transfers first, then NFT transfers
  const items: React.ReactNode[] = [];
  let remaining = TRANSFERS_LIMIT;
  for (let i = 0; i < transfers.length && remaining > 0; i++, remaining--) {
    items.push(<TransferSummary key={`ft-${i}`} transfer={transfers[i]} />);
  }
  for (let i = 0; i < nftTransfers.length && remaining > 0; i++, remaining--) {
    items.push(
      <NftTransferSummary key={`nft-${i}`} transfer={nftTransfers[i]} />,
    );
  }

  return (
    <>
      {items}
      <button
        onClick={() => setExpanded(true)}
        className="cursor-pointer text-blue-600 hover:text-blue-800 text-left"
      >
        + {totalCount - TRANSFERS_LIMIT} more...
      </button>
    </>
  );
}

export interface TxTableItem {
  tx: ParsedTx;
  timestamp: string;
}

export function TxTableHeader() {
  const th = "px-4 py-3 whitespace-nowrap";
  return (
    <thead>
      <tr className="border-b border-gray-200 bg-gray-50 text-left text-xs font-medium uppercase text-gray-500">
        <th className={th}>Tx Hash</th>
        <th className={th}>Time</th>
        <th className={th}>Signer</th>
        <th className={th}>Receiver</th>
        <th className={th}>Action</th>
        <th className={th}>Gas</th>
        <th className={th}></th>
      </tr>
    </thead>
  );
}

export default function TxRow({ tx, timestamp }: TxTableItem) {
  const hasTransfers = tx.transfers.length > 0 || tx.nftTransfers.length > 0;
  const [popupActionIndex, setPopupActionIndex] = useState<number | null>(null);
  const closePopup = useCallback(() => setPopupActionIndex(null), []);

  return (
    <tbody className="group">
      <tr className={`border-b border-gray-100 group-hover:bg-gray-50 ${hasTransfers ? "border-b-0" : ""}`}>
        <td className="whitespace-nowrap px-4 py-3">
          <TransactionHash hash={tx.hash} />
        </td>
        <td className="whitespace-nowrap px-4 py-3 text-gray-500">
          <TimeAgo timestampNs={timestamp} />
        </td>
        <td className="px-4 py-3">
          <span className="inline-flex items-center gap-1">
            {tx.relayer_id && (
              <Link
                to={`/account/${tx.relayer_id}`}
                title={`Relayed by ${tx.relayer_id}`}
              >
                <Radio className="size-3.5 text-red-500" />
              </Link>
            )}
            <AccountId accountId={tx.signer_id} maxLength={20} />
          </span>
        </td>
        <td className="px-4 py-3">
          <AccountId accountId={tx.receiver_id} maxLength={20} />
        </td>
        <td className="whitespace-nowrap px-4 py-3 font-mono text-xs">
          <ActionList actions={tx.actions} onActionClick={setPopupActionIndex} />
        </td>
        <td className="whitespace-nowrap px-4 py-3 text-xs">
          <GasAmount gas={tx.gas_burnt} />
        </td>
        <td className="whitespace-nowrap px-4 py-3">
          {tx.is_success === null ? (
            <Clock className="size-4 text-yellow-500" />
          ) : tx.is_success ? (
            <CircleCheck className="size-4 text-green-600" />
          ) : (
            <CircleX className="size-4 text-red-600" />
          )}
        </td>
      </tr>
      {hasTransfers && (
        <tr className="border-b border-gray-100 group-hover:bg-gray-50">
          <td colSpan={7} className="pb-2 pt-0 pl-10 pr-4">
            <div className="flex flex-col gap-0.5 text-xs text-gray-600 dark:text-gray-400">
              <TransferList transfers={tx.transfers} nftTransfers={tx.nftTransfers} />
            </div>
          </td>
        </tr>
      )}
      {popupActionIndex !== null && (
        <ReceiptPopup
          txHash={tx.hash}
          receipts={tx.receipts}
          action={tx.actions[popupActionIndex]}
          actionIndex={popupActionIndex}
          onClose={closePopup}
        />
      )}
    </tbody>
  );
}

function TxMobileCard({ tx, timestamp }: TxTableItem) {
  const [popupActionIndex, setPopupActionIndex] = useState<number | null>(null);
  const closePopup = useCallback(() => setPopupActionIndex(null), []);

  return (
    <div className="overflow-hidden px-3 py-2.5">
      <div className="flex items-center justify-between gap-2 mb-1">
        <div className="flex items-center gap-1.5 min-w-0">
          {tx.is_success === null ? (
            <Clock className="size-3.5 text-yellow-500 shrink-0" />
          ) : tx.is_success ? (
            <CircleCheck className="size-3.5 text-green-600 shrink-0" />
          ) : (
            <CircleX className="size-3.5 text-red-600 shrink-0" />
          )}
          <span className="font-mono text-xs truncate">
            <ActionList actions={tx.actions} onActionClick={setPopupActionIndex} />
          </span>
        </div>
        <span className="text-xs text-gray-500 shrink-0">
          <TimeAgo timestampNs={timestamp} />
        </span>
      </div>
      <div className="mb-1 text-sm">
        <TransactionHash hash={tx.hash} />
      </div>
      <div className="flex flex-wrap items-center gap-x-1 gap-y-0.5 text-sm min-w-0">
        <span className="inline-flex items-center gap-1 min-w-0 max-w-full">
          {tx.relayer_id && (
            <Link
              to={`/account/${tx.relayer_id}`}
              title={`Relayed by ${tx.relayer_id}`}
            >
              <Radio className="size-3 text-red-500 shrink-0" />
            </Link>
          )}
          <AccountId accountId={tx.signer_id} maxLength="auto" />
        </span>
        <span className="inline-flex items-center gap-1 min-w-0 max-w-full">
          <span className="text-gray-400 shrink-0">→</span>
          <AccountId accountId={tx.receiver_id} maxLength="auto" />
        </span>
      </div>
      <div className="text-xs text-gray-500 mt-0.5">
        <GasAmount gas={tx.gas_burnt} />
      </div>
      {(tx.transfers.length > 0 || tx.nftTransfers.length > 0) && (
        <div className="flex flex-col gap-0.5 text-xs text-gray-600 dark:text-gray-400 mt-1 pt-1 border-t border-gray-100">
          <TransferList transfers={tx.transfers} nftTransfers={tx.nftTransfers} />
        </div>
      )}
      {popupActionIndex !== null && (
        <ReceiptPopup
          txHash={tx.hash}
          receipts={tx.receipts}
          action={tx.actions[popupActionIndex]}
          actionIndex={popupActionIndex}
          onClose={closePopup}
        />
      )}
    </div>
  );
}

export function TxTable({ items }: { items: TxTableItem[] }) {
  return (
    <>
      <div className="hidden sm:block min-w-fit rounded-lg border border-gray-200 bg-surface">
        <table className="w-full text-sm">
          <TxTableHeader />
          {items.map((item, i) => (
            <TxRow
              key={`${item.tx.hash}-${i}`}
              tx={item.tx}
              timestamp={item.timestamp}
            />
          ))}
        </table>
      </div>
      <div className="sm:hidden rounded-lg border border-gray-200 bg-surface divide-y divide-gray-100">
        {items.map((item, i) => (
          <TxMobileCard
            key={`${item.tx.hash}-${i}`}
            tx={item.tx}
            timestamp={item.timestamp}
          />
        ))}
      </div>
    </>
  );
}

function SpamFilterBar({
  spamCount,
  showSpam,
  setShowSpam,
}: {
  spamCount: number;
  showSpam: boolean;
  setShowSpam: (v: boolean) => void;
}) {
  const [open, setOpen] = useState(false);

  return (
    <div className="border-b border-gray-200">
      <div className="flex items-center justify-between gap-2 px-4 py-2">
        <button
          onClick={() => setOpen((o) => !o)}
          className="inline-flex cursor-pointer items-center gap-1.5 rounded px-2 py-1 text-xs font-medium text-gray-500 hover:bg-gray-100 hover:text-gray-700"
        >
          <SlidersHorizontal className="size-3.5" />
          Filters
        </button>
        {!showSpam && spamCount > 0 && (
          <span className="text-xs text-gray-400">
            Hiding {spamCount} spam
          </span>
        )}
      </div>
      {open && (
        <div className="border-t border-gray-100 bg-gray-50 px-4 py-3">
          <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-gray-400">
            Spam
          </div>
          <label className="inline-flex cursor-pointer items-center gap-2 text-xs text-gray-600">
            <input
              type="checkbox"
              checked={showSpam}
              onChange={(e) => setShowSpam(e.target.checked)}
              className="rounded"
            />
            Show spam transactions
            {spamCount > 0 && (
              <span className="text-gray-400">({spamCount} hidden)</span>
            )}
          </label>
        </div>
      )}
    </div>
  );
}

export interface SpamFilterProps {
  showSpam: boolean;
  setShowSpam: (v: boolean) => void;
  spamCount: number;
}

export function FilteredTxTable({
  items,
  filterBar,
}: {
  items: TxTableItem[];
  filterBar?: (spam: SpamFilterProps) => React.ReactNode;
}) {
  const [showSpam, setShowSpam] = useState(false);
  const spamTokens = useSpamTokens();
  const spamNfts = useSpamNfts();

  const { filtered, spamCount } = useMemo(() => {
    if (showSpam) return { filtered: items, spamCount: 0 };
    let spamCount = 0;
    const filtered = items.filter((item) => {
      const { signer_id, receiver_id } = item.tx;
      const spam =
        isSpam(spamTokens, signer_id) ||
        isSpam(spamTokens, receiver_id) ||
        isSpamNft(spamNfts, signer_id) ||
        isSpamNft(spamNfts, receiver_id);
      if (spam) spamCount++;
      return !spam;
    });
    return { filtered, spamCount };
  }, [items, showSpam, spamTokens, spamNfts]);

  const spamProps: SpamFilterProps = { showSpam, setShowSpam, spamCount };
  const topBar = filterBar
    ? filterBar(spamProps)
    : <SpamFilterBar {...spamProps} />;

  return (
    <>
      <div className="hidden sm:block min-w-fit rounded-lg border border-gray-200 bg-surface">
        {topBar}
        <table className="w-full text-sm">
          <TxTableHeader />
          {filtered.map((item, i) => (
            <TxRow
              key={`${item.tx.hash}-${i}`}
              tx={item.tx}
              timestamp={item.timestamp}
            />
          ))}
        </table>
      </div>
      <div className="sm:hidden rounded-lg border border-gray-200 bg-surface divide-y divide-gray-100">
        {topBar}
        {filtered.map((item, i) => (
          <TxMobileCard
            key={`${item.tx.hash}-${i}`}
            tx={item.tx}
            timestamp={item.timestamp}
          />
        ))}
      </div>
    </>
  );
}
