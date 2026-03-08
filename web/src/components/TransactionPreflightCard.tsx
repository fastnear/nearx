import type { ReactNode } from "react";

type PreflightTone = "neutral" | "success" | "warning" | "danger";

interface TransactionPreflightCardProps {
  statusLabel: string;
  statusTone?: PreflightTone;
  items: Array<{
    label: string;
    value: ReactNode;
  }>;
  argsPreview?: string | null;
}

function toneClass(tone: PreflightTone): string {
  switch (tone) {
    case "success":
      return "border-emerald-200 bg-emerald-50 text-emerald-700";
    case "warning":
      return "border-amber-200 bg-amber-50 text-amber-700";
    case "danger":
      return "border-rose-200 bg-rose-50 text-rose-700";
    default:
      return "border-slate-200 bg-slate-50 text-slate-700";
  }
}

export default function TransactionPreflightCard({
  statusLabel,
  statusTone = "neutral",
  items,
  argsPreview,
}: TransactionPreflightCardProps) {
  return (
    <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-gray-100 px-4 py-4">
        <div>
          <h2 className="text-base font-medium text-gray-900">Review transaction</h2>
          <div className="mt-1 text-sm text-gray-500">
            Confirm the signer, receiver, action, and units before you sign.
          </div>
        </div>
        <span
          className={`inline-flex rounded-full border px-2.5 py-1 text-xs font-semibold uppercase tracking-wide ${toneClass(statusTone)}`}
        >
          {statusLabel}
        </span>
      </div>
      <dl className="grid gap-x-8 gap-y-4 px-4 py-4 sm:grid-cols-2">
        {items.map((item) => (
          <div key={item.label}>
            <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
              {item.label}
            </dt>
            <dd className="mt-1 text-base text-gray-900">{item.value}</dd>
          </div>
        ))}
      </dl>
      {argsPreview ? (
        <div className="border-t border-gray-100 px-4 py-4">
          <div className="mb-2 text-xs font-medium uppercase tracking-wide text-gray-500">
            Args Preview
          </div>
          <pre className="overflow-x-auto whitespace-pre-wrap rounded bg-gray-50 p-3 text-sm text-gray-700">
            {argsPreview}
          </pre>
        </div>
      ) : null}
    </div>
  );
}
