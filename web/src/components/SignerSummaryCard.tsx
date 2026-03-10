import type { ReactNode } from "react";

type SummaryTone = "neutral" | "success" | "warning" | "danger";

interface SummaryItem {
  label: string;
  value: ReactNode;
}

interface SignerSummaryCardProps {
  title: string;
  statusLabel: string;
  statusTone?: SummaryTone;
  items: SummaryItem[];
  message?: ReactNode;
  actions?: ReactNode;
  controls?: ReactNode;
}

function statusClass(tone: SummaryTone): string {
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

export default function SignerSummaryCard({
  title,
  statusLabel,
  statusTone = "neutral",
  items,
  message,
  actions,
  controls,
}: SignerSummaryCardProps) {
  return (
    <div className="mb-4 rounded-lg border border-gray-200 bg-surface shadow-sm text-sm">
      <div className="border-b border-gray-100 px-4 py-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-base font-medium text-gray-900">{title}</h2>
          <div className="flex flex-wrap items-center justify-end gap-2">
            {actions}
            <span
              className={`inline-flex rounded-full border px-2.5 py-1 text-xs font-semibold ${statusClass(statusTone)}`}
            >
              {statusLabel}
            </span>
          </div>
        </div>
        {message ? <div className="mt-2 text-sm text-gray-500">{message}</div> : null}
      </div>
      {controls}
      <dl className="grid grid-cols-2 gap-x-8 gap-y-3 px-4 py-4 sm:grid-cols-3">
        {items.map((item) => (
          <div key={item.label} className="min-w-0">
            <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
              {item.label}
            </dt>
            <dd className="mt-1 min-h-[1.5rem] min-w-0 break-words text-sm text-gray-900">
              {item.value}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
