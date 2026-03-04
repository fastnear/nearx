import { useEffect, useState } from "react";
import type { ReceiptWithOutcome } from "../api/types";
import { parseAction } from "../utils/parseTransaction";
import GasAmount from "./GasAmount";
import NearAmount from "./NearAmount";
import AccountId from "./AccountId";
import BlockHeight from "./BlockHeight";
import TimeAgo from "./TimeAgo";
import { ActionExpanded } from "./Action";
import Base64Data from "./Base64Data";
import JsonView from "@uiw/react-json-view";
import { darkTheme } from "@uiw/react-json-view/dark";
import { CircleCheck, CircleX } from "lucide-react";

function useIsDark() {
  const [dark, setDark] = useState(() =>
    document.documentElement.classList.contains("dark")
  );
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setDark(document.documentElement.classList.contains("dark"));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => observer.disconnect();
  }, []);
  return dark;
}

export function EventLog({ log }: { log: string }) {
  const isDark = useIsDark();
  if (!log.startsWith("EVENT_JSON:")) {
    return (
      <div className="whitespace-pre-wrap break-all rounded bg-gray-50 p-2 font-mono">
        {log}
      </div>
    );
  }
  try {
    const json = JSON.parse(log.slice("EVENT_JSON:".length));
    return (
      <div className="rounded bg-gray-50 p-2">
        <span className="font-semibold text-purple-700">
          {json.standard ?? "event"}
        </span>
        {json.event && (
          <span className="ml-1 text-gray-600">: {json.event}</span>
        )}
        {json.data && (
          <div className="mt-1 overflow-auto text-xs">
            <JsonView
              value={json.data}
              collapsed={4}
              displayDataTypes={false}
              displayObjectSize={false}
              shortenTextAfterLength={512}
              style={isDark ? darkTheme : undefined}
            />
          </div>
        )}
      </div>
    );
  } catch {
    return (
      <div className="whitespace-pre-wrap break-all rounded bg-gray-50 p-2 font-mono">
        {log}
      </div>
    );
  }
}

export function ReceiptResult({
  status,
  txHash,
}: {
  status: Record<string, unknown>;
  txHash?: string;
}) {
  const isDark = useIsDark();
  if ("SuccessReceiptId" in status) {
    const receiptId = String(status.SuccessReceiptId);
    const href = txHash
      ? `/tx/${txHash}#receipt-${receiptId}`
      : `#receipt-${receiptId}`;
    return (
      <div className="overflow-auto rounded border border-gray-200 bg-gray-50 p-2 text-xs font-mono break-all">
        Receipt:{" "}
        <a
          href={href}
          className="text-blue-600 hover:underline"
        >
          {receiptId}
        </a>
      </div>
    );
  }
  if ("SuccessValue" in status) {
    return <Base64Data base64={String(status.SuccessValue)} />;
  }
  if ("Failure" in status) {
    return (
      <div className="overflow-auto rounded border border-gray-200 bg-red-50 p-2 text-xs text-red-700">
        <JsonView
          value={status.Failure as object}
          collapsed={2}
          displayDataTypes={false}
          displayObjectSize={false}
          style={isDark ? darkTheme : undefined}
        />
      </div>
    );
  }
  return null;
}

export default function ReceiptCard({ r, txHash }: { r: ReceiptWithOutcome; txHash?: string }) {
  const outcome = r.execution_outcome.outcome;
  const statusKey = Object.keys(outcome.status)[0];
  const isSuccess =
    statusKey === "SuccessValue" || statusKey === "SuccessReceiptId";

  const actions: unknown[] =
    (r.receipt.receipt as Record<string, unknown>)?.Action
      ? (((r.receipt.receipt as Record<string, unknown>).Action as Record<string, unknown>)?.actions as unknown[]) ?? []
      : [];

  return (
    <div
      id={`receipt-${r.receipt.receipt_id}`}
      className="rounded-lg border border-gray-200 bg-surface text-sm"
    >
      <dl className="grid gap-px sm:grid-cols-2 [&>div]:flex [&>div]:items-center [&>div]:gap-2 [&>div]:border-b [&>div]:border-gray-100 [&>div]:px-4 [&>div]:py-2 [&>div:last-child]:border-b-0 [&>div:nth-last-child(2)]:sm:border-b-0">
        <div className="sm:col-span-2">
          <dt className="shrink-0 text-gray-500">ID</dt>
          <dd className="flex flex-1 min-w-0 items-center justify-between gap-2">
            <span className="font-mono text-xs text-gray-400 break-all">
              {r.receipt.receipt_id}
            </span>
            <span className="flex shrink-0 items-center gap-1">
              {isSuccess ? (
                <CircleCheck className="size-3.5 text-green-600" />
              ) : (
                <CircleX className="size-3.5 text-red-600" />
              )}
            </span>
          </dd>
        </div>
        <div>
          <dt className="shrink-0 text-gray-500">Predecessor</dt>
          <dd>
            <AccountId accountId={r.receipt.predecessor_id} />
          </dd>
        </div>
        <div>
          <dt className="shrink-0 text-gray-500">Receiver</dt>
          <dd>
            <AccountId accountId={r.receipt.receiver_id} />
          </dd>
        </div>
        <div>
          <dt className="shrink-0 text-gray-500">Block</dt>
          <dd>
            <BlockHeight height={r.execution_outcome.block_height} />
          </dd>
        </div>
        <div>
          <dt className="shrink-0 text-gray-500">Time</dt>
          <dd>
            <TimeAgo timestampNs={String(r.receipt.block_timestamp)} />
          </dd>
        </div>
        <div>
          <dt className="shrink-0 text-gray-500">Gas Burnt</dt>
          <dd>
            <GasAmount gas={outcome.gas_burnt} />
          </dd>
        </div>
        <div>
          <dt className="shrink-0 text-gray-500">Tokens Burnt</dt>
          <dd>
            <NearAmount yoctoNear={outcome.tokens_burnt} />
          </dd>
        </div>
      </dl>

      {actions.length > 0 && (
        <div className="border-t border-gray-100 px-4 py-2">
          <p className="mb-1 text-xs font-medium text-gray-500">Actions</p>
          <div className="space-y-2 font-mono text-xs">
            {actions.map((a, i) => (
              <ActionExpanded
                key={i}
                action={parseAction(a as Record<string, unknown>)}
              />
            ))}
          </div>
        </div>
      )}

      <div className="border-t border-gray-100 px-4 py-2">
        <p className="mb-1 text-xs font-medium text-gray-500">Result</p>
        <ReceiptResult status={outcome.status} txHash={txHash} />
      </div>

      {outcome.logs.length > 0 && (
        <div className="border-t border-gray-100 px-4 py-2">
          <p className="mb-1 text-xs font-medium text-gray-500">Logs</p>
          <div className="space-y-1 text-xs">
            {outcome.logs.map((log, i) => (
              <EventLog key={i} log={log} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
