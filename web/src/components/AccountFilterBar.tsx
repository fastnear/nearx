import { useState } from "react";
import type { AccountFilters } from "../api/types";
import type { SpamFilterProps } from "./TxRow";
import { SlidersHorizontal, X } from "lucide-react";

type TriState = boolean | undefined;

function cycleTriState(value: TriState): TriState {
  if (value === undefined) return true;
  if (value === true) return false;
  return undefined;
}

function FilterChip({
  label,
  value,
  onClick,
}: {
  label: string;
  value: TriState;
  onClick: () => void;
}) {
  let bg: string;
  let text: string;
  if (value === true) {
    bg = "bg-green-100 border-green-300 hover:bg-green-200";
    text = "text-green-800";
  } else if (value === false) {
    bg = "bg-red-100 border-red-300 hover:bg-red-200";
    text = "text-red-800";
  } else {
    bg = "bg-gray-100 border-gray-200 hover:bg-gray-200";
    text = "text-gray-600";
  }

  return (
    <button
      type="button"
      onClick={onClick}
      className={`inline-flex cursor-pointer items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs font-medium ${bg} ${text}`}
    >
      {label}
      {value !== undefined && (
        <span className="text-[10px]">{value ? "Yes" : "No"}</span>
      )}
    </button>
  );
}

interface FilterGroup {
  label: string;
  filters: { key: keyof AccountFilters; label: string }[];
}

const FILTER_GROUPS: FilterGroup[] = [
  {
    label: "Role",
    filters: [
      { key: "is_signer", label: "Signer" },
      { key: "is_real_signer", label: "Real Signer" },
      { key: "is_delegated_signer", label: "Delegated Signer" },
      { key: "is_any_signer", label: "Any Signer" },
      { key: "is_receiver", label: "Receiver" },
      { key: "is_real_receiver", label: "Real Receiver" },
      { key: "is_predecessor", label: "Predecessor" },
      { key: "is_explicit_refund_to", label: "Refund Recipient" },
    ],
  },
  {
    label: "Type",
    filters: [
      { key: "is_function_call", label: "Function Call" },
      { key: "is_action_arg", label: "Action Arg" },
      { key: "is_event_log", label: "Event Log" },
    ],
  },
  {
    label: "Status",
    filters: [{ key: "is_success", label: "Successful" }],
  },
];

export default function AccountFilterBar({
  filters,
  onChange,
  hasActiveFilters,
  spam,
}: {
  filters: AccountFilters;
  onChange: (filters: AccountFilters) => void;
  hasActiveFilters: boolean;
  spam: SpamFilterProps;
}) {
  const [open, setOpen] = useState(false);

  const toggleFilter = (key: keyof AccountFilters) => {
    const current = filters[key] as TriState;
    const next = cycleTriState(current);
    const updated = { ...filters };
    if (next === undefined) {
      delete updated[key];
    } else {
      (updated as Record<string, boolean>)[key] = next;
    }
    onChange(updated);
  };

  const setNumberFilter = (key: "from_tx_block_height" | "to_tx_block_height", value: string) => {
    const updated = { ...filters };
    if (value === "") {
      delete updated[key];
    } else {
      const num = Number(value);
      if (!isNaN(num) && num >= 0) updated[key] = num;
    }
    onChange(updated);
  };

  const toggleOrder = () => {
    const updated = { ...filters };
    if (filters.desc === false) {
      delete updated.desc;
    } else {
      updated.desc = false;
    }
    onChange(updated);
  };

  const clearAll = () => onChange({});

  return (
    <div className="border-b border-gray-200">
      <div className="flex items-center justify-between gap-2 px-4 py-2">
        <div className="flex items-center gap-2">
          <button
            onClick={() => setOpen((o) => !o)}
            className={`inline-flex cursor-pointer items-center gap-1.5 rounded px-2 py-1 text-xs font-medium ${
              hasActiveFilters
                ? "bg-blue-50 text-blue-700 hover:bg-blue-100"
                : "text-gray-500 hover:bg-gray-100 hover:text-gray-700"
            }`}
          >
            <SlidersHorizontal className="size-3.5" />
            Filters
            {hasActiveFilters && (
              <span className="rounded-full bg-blue-200 px-1.5 text-[10px] font-semibold text-blue-800">
                {Object.keys(filters).length}
              </span>
            )}
          </button>
          {hasActiveFilters && (
            <button
              onClick={clearAll}
              className="inline-flex cursor-pointer items-center gap-1 rounded px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 hover:text-gray-700"
            >
              <X className="size-3" />
              Clear all
            </button>
          )}
        </div>
        <div className="flex items-center gap-3">
          {!spam.showSpam && spam.spamCount > 0 && (
            <span className="text-xs text-gray-400">
              Hiding {spam.spamCount} spam
            </span>
          )}
          <button
            onClick={toggleOrder}
            className="inline-flex cursor-pointer items-center gap-1 rounded px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 hover:text-gray-700"
          >
            {filters.desc === false ? "Oldest first" : "Newest first"}
          </button>
        </div>
      </div>
      {open && (
        <div className="border-t border-gray-100 bg-gray-50 px-4 py-3 space-y-3">
          {FILTER_GROUPS.map((group) => (
            <div key={group.label}>
              <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-gray-400">
                {group.label}
              </div>
              <div className="flex flex-wrap gap-1.5">
                {group.filters.map(({ key, label }) => (
                  <FilterChip
                    key={key}
                    label={label}
                    value={filters[key] as TriState}
                    onClick={() => toggleFilter(key)}
                  />
                ))}
              </div>
            </div>
          ))}
          <div>
            <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-gray-400">
              Block Height Range
            </div>
            <div className="flex items-center gap-2">
              <input
                type="number"
                placeholder="From"
                defaultValue={filters.from_tx_block_height ?? ""}
                onBlur={(e) => setNumberFilter("from_tx_block_height", e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                }}
                className="w-36 rounded border border-gray-200 bg-white px-2 py-1 text-xs focus:border-blue-300 focus:outline-none"
              />
              <span className="text-xs text-gray-400">to</span>
              <input
                type="number"
                placeholder="To"
                defaultValue={filters.to_tx_block_height ?? ""}
                onBlur={(e) => setNumberFilter("to_tx_block_height", e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                }}
                className="w-36 rounded border border-gray-200 bg-white px-2 py-1 text-xs focus:border-blue-300 focus:outline-none"
              />
            </div>
          </div>
          <div>
            <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-gray-400">
              Spam
            </div>
            <label className="inline-flex cursor-pointer items-center gap-2 text-xs text-gray-600">
              <input
                type="checkbox"
                checked={spam.showSpam}
                onChange={(e) => spam.setShowSpam(e.target.checked)}
                className="rounded"
              />
              Show spam transactions
              {spam.spamCount > 0 && (
                <span className="text-gray-400">({spam.spamCount} hidden)</span>
              )}
            </label>
          </div>
        </div>
      )}
    </div>
  );
}
