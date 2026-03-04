import { useState } from "react";
import type { BlockFilters } from "../api/types";
import { SlidersHorizontal, X } from "lucide-react";

export default function BlockFilterBar({
  filters,
  onChange,
  hasActiveFilters,
}: {
  filters: BlockFilters;
  onChange: (filters: BlockFilters) => void;
  hasActiveFilters: boolean;
}) {
  const [open, setOpen] = useState(false);

  const setNumberFilter = (key: "from_block_height" | "to_block_height", value: string) => {
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
        <button
          onClick={toggleOrder}
          className="inline-flex cursor-pointer items-center gap-1 rounded px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 hover:text-gray-700"
        >
          {filters.desc === false ? "Oldest first" : "Newest first"}
        </button>
      </div>
      {open && (
        <div className="border-t border-gray-100 bg-gray-50 px-4 py-3">
          <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-gray-400">
            Block Height Range
          </div>
          <div className="flex items-center gap-2">
            <input
              type="number"
              placeholder="From"
              defaultValue={filters.from_block_height ?? ""}
              onBlur={(e) => setNumberFilter("from_block_height", e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              }}
              className="w-36 rounded border border-gray-200 bg-white px-2 py-1 text-xs focus:border-blue-300 focus:outline-none"
            />
            <span className="text-xs text-gray-400">to</span>
            <input
              type="number"
              placeholder="To"
              defaultValue={filters.to_block_height ?? ""}
              onBlur={(e) => setNumberFilter("to_block_height", e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              }}
              className="w-36 rounded border border-gray-200 bg-white px-2 py-1 text-xs focus:border-blue-300 focus:outline-none"
            />
          </div>
        </div>
      )}
    </div>
  );
}
