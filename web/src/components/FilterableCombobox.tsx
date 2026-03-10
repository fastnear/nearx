import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ChevronDown, Star } from "lucide-react";

interface ComboboxOption {
  value: string;
  label: string;
}

interface FilterableComboboxProps {
  label: string;
  value: string;
  options: ComboboxOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  placeholder?: string;
  starredValues?: Set<string>;
  onToggleStar?: (value: string) => void;
}

function highlightMatch(text: string, query: string): ReactNode {
  if (!query) return text;
  const lower = text.toLowerCase();
  const idx = lower.indexOf(query.toLowerCase());
  if (idx === -1) return text;
  return (
    <>
      {text.slice(0, idx)}
      <mark className="bg-yellow-200/60 text-inherit">{text.slice(idx, idx + query.length)}</mark>
      {text.slice(idx + query.length)}
    </>
  );
}

const baseClass =
  "w-full min-w-0 rounded-md border border-gray-300 bg-surface px-3 py-2 text-sm text-gray-900 focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20 focus:outline-none";

export default function FilterableCombobox({
  label: fieldLabel,
  value,
  options,
  onChange,
  disabled = false,
  placeholder = "Not available",
  starredValues,
  onToggleStar,
}: FilterableComboboxProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [highlightedIndex, setHighlightedIndex] = useState(0);

  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listboxRef = useRef<HTMLUListElement>(null);

  const hasOptions = options.length > 0;
  const selectedOption = options.find((o) => o.value === value);
  const displayLabel = selectedOption?.label ?? placeholder;

  const filtered = useMemo(() => {
    if (!query) return options;
    const q = query.toLowerCase();
    return options.filter(
      (o) => o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q),
    );
  }, [options, query]);

  const listboxId = `combobox-listbox-${fieldLabel.replace(/\s+/g, "-").toLowerCase()}`;

  const openDropdown = useCallback(() => {
    if (disabled || !hasOptions) return;
    setQuery("");
    setOpen(true);
    const idx = options.findIndex((o) => o.value === value);
    setHighlightedIndex(idx >= 0 ? idx : 0);
    requestAnimationFrame(() => inputRef.current?.focus());
  }, [disabled, hasOptions, options, value]);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
  }, []);

  const selectOption = useCallback(
    (val: string) => {
      onChange(val);
      close();
    },
    [onChange, close],
  );

  // Click-outside
  useEffect(() => {
    if (!open) return;
    function handleMouseDown(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        close();
      }
    }
    document.addEventListener("mousedown", handleMouseDown);
    return () => document.removeEventListener("mousedown", handleMouseDown);
  }, [open, close]);

  // Scroll highlighted into view
  useEffect(() => {
    if (!open || !listboxRef.current) return;
    const item = listboxRef.current.children[highlightedIndex] as HTMLElement | undefined;
    if (item && typeof item.scrollIntoView === "function") {
      item.scrollIntoView({ block: "nearest" });
    }
  }, [open, highlightedIndex]);

  // Clamp highlight when filter changes
  useEffect(() => {
    if (highlightedIndex >= filtered.length) {
      setHighlightedIndex(Math.max(0, filtered.length - 1));
    }
  }, [filtered.length, highlightedIndex]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        openDropdown();
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setHighlightedIndex((i) => (i + 1) % filtered.length);
        break;
      case "ArrowUp":
        e.preventDefault();
        setHighlightedIndex((i) => (i - 1 + filtered.length) % filtered.length);
        break;
      case "Home":
        e.preventDefault();
        setHighlightedIndex(0);
        break;
      case "End":
        e.preventDefault();
        setHighlightedIndex(filtered.length - 1);
        break;
      case "Enter":
        e.preventDefault();
        if (filtered[highlightedIndex]) {
          selectOption(filtered[highlightedIndex].value);
        }
        break;
      case "Escape":
        e.preventDefault();
        close();
        break;
    }
  }

  const highlightedOptionId =
    filtered[highlightedIndex] && open
      ? `${listboxId}-option-${highlightedIndex}`
      : undefined;

  if (disabled || !hasOptions) {
    return (
      <div className={`${baseClass} cursor-not-allowed opacity-50`}>
        {placeholder}
      </div>
    );
  }

  return (
    <div ref={containerRef} className="relative" onKeyDown={handleKeyDown}>
      {open ? (
        <input
          ref={inputRef}
          type="text"
          role="combobox"
          aria-expanded={true}
          aria-haspopup="listbox"
          aria-controls={listboxId}
          aria-activedescendant={highlightedOptionId}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Type to filter…"
          className={baseClass}
        />
      ) : (
        <button
          type="button"
          role="combobox"
          aria-expanded={false}
          aria-haspopup="listbox"
          onClick={openDropdown}
          className={`${baseClass} flex cursor-pointer items-center gap-2 text-left`}
        >
          {starredValues?.has(value) && (
            <Star size={12} className="shrink-0 fill-yellow-400 text-yellow-400" />
          )}
          <span className="min-w-0 flex-1 break-all">{displayLabel}</span>
          <ChevronDown size={14} className="shrink-0 text-gray-400" />
        </button>
      )}
      {open && (
        <ul
          ref={listboxRef}
          id={listboxId}
          role="listbox"
          className="absolute z-20 mt-1 max-h-48 w-full overflow-y-auto rounded-md border border-gray-200 bg-surface shadow-lg"
        >
          {filtered.length === 0 ? (
            <li className="px-3 py-2 text-sm text-gray-400">No matches</li>
          ) : (
            filtered.map((opt, i) => (
              <li
                key={opt.value}
                id={`${listboxId}-option-${i}`}
                role="option"
                aria-selected={opt.value === value}
                onMouseDown={(e) => {
                  e.preventDefault();
                  selectOption(opt.value);
                }}
                onMouseEnter={() => setHighlightedIndex(i)}
                className={`flex cursor-pointer items-center gap-1.5 break-all px-3 py-2 text-sm ${
                  i === highlightedIndex
                    ? "bg-blue-600 text-white"
                    : "text-gray-900 hover:bg-gray-100"
                } ${opt.value === value ? "font-semibold" : ""}`}
              >
                {onToggleStar && (
                  <button
                    type="button"
                    onMouseDown={(e) => {
                      e.stopPropagation();
                      e.preventDefault();
                      onToggleStar(opt.value);
                    }}
                    className="shrink-0 p-0.5"
                  >
                    <Star
                      size={12}
                      className={
                        starredValues?.has(opt.value)
                          ? "fill-yellow-400 text-yellow-400"
                          : i === highlightedIndex
                            ? "text-white/50"
                            : "text-gray-300"
                      }
                    />
                  </button>
                )}
                <span className="min-w-0 flex-1">{highlightMatch(opt.label, query)}</span>
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}
