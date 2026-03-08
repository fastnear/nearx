import { useEffect, useId, useState } from "react";

const LEDGER_DERIVATION_PATH_PREFIX = "44'/397'/0'/0'/";
const COMMON_LEDGER_PATH_SLOTS = ["1", "2", "3", "4", "5", "10"];

function parseLedgerSlot(path: string): string | null {
  const trimmed = path.trim();
  const match = trimmed.match(/^44'\/397'\/0'\/0'\/(\d+)'?$/);
  return match?.[1] ?? null;
}

function normalizeSlotInput(raw: string): string {
  return raw.replace(/[^\d]/g, "").replace(/^0+(?=\d)/, "");
}

function buildLedgerPath(slot: string): string {
  return `${LEDGER_DERIVATION_PATH_PREFIX}${slot}'`;
}

interface LedgerPathFieldProps {
  value: string;
  onChange: (path: string) => void;
  disabled?: boolean;
}

export default function LedgerPathField({
  value,
  onChange,
  disabled = false,
}: LedgerPathFieldProps) {
  const matchedSlot = parseLedgerSlot(value);
  const [customMode, setCustomMode] = useState(matchedSlot === null);
  const [slotValue, setSlotValue] = useState(matchedSlot ?? "1");
  const datalistId = useId();

  useEffect(() => {
    if (matchedSlot !== null) {
      setCustomMode(false);
      setSlotValue(matchedSlot);
      return;
    }
    if (value.trim()) {
      setCustomMode(true);
    }
  }, [matchedSlot, value]);

  function setSlot(nextRaw: string) {
    const normalized = normalizeSlotInput(nextRaw);
    setSlotValue(normalized);
    if (!normalized) return;
    onChange(buildLedgerPath(normalized));
  }

  function toggleCustomMode() {
    if (customMode) {
      const fallback = normalizeSlotInput(slotValue) || "1";
      setCustomMode(false);
      setSlotValue(fallback);
      onChange(buildLedgerPath(fallback));
      return;
    }
    setCustomMode(true);
  }

  return (
    <div className="flex flex-wrap items-center gap-2 text-xs text-gray-600">
      {!customMode ? (
        <>
          <span className="text-gray-500">Assigned path:</span>
          <code className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-[11px] text-gray-700">
            {LEDGER_DERIVATION_PATH_PREFIX}
          </code>
          <input
            type="text"
            inputMode="numeric"
            pattern="[0-9]*"
            list={datalistId}
            value={slotValue}
            onChange={(e) => setSlot(e.target.value)}
            disabled={disabled}
            className="w-16 rounded border border-gray-200 bg-gray-50 px-2 py-1 font-mono text-[11px] text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:outline-none disabled:opacity-50"
            aria-label="Ledger path slot"
            placeholder="1"
          />
          <datalist id={datalistId}>
            {COMMON_LEDGER_PATH_SLOTS.map((slot) => (
              <option key={slot} value={slot} />
            ))}
          </datalist>
          <code className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-[11px] text-gray-700">
            '
          </code>
          <span className="text-gray-500">
            Full path:{" "}
            <code className="font-mono text-[11px] text-gray-700">
              {buildLedgerPath(normalizeSlotInput(slotValue) || "1")}
            </code>
          </span>
          <div className="flex flex-wrap gap-1">
            {COMMON_LEDGER_PATH_SLOTS.map((slot) => (
              <button
                key={slot}
                type="button"
                onClick={() => setSlot(slot)}
                disabled={disabled}
                className={`rounded border px-1.5 py-0.5 font-mono text-[11px] ${
                  slot === (normalizeSlotInput(slotValue) || "1")
                    ? "border-blue-300 bg-blue-50 text-blue-700"
                    : "border-gray-200 text-gray-600 hover:bg-gray-50"
                } disabled:opacity-50`}
              >
                {slot}
              </button>
            ))}
          </div>
        </>
      ) : (
        <>
          <span className="text-gray-500">Custom path:</span>
          <input
            type="text"
            value={value}
            onChange={(e) => onChange(e.target.value)}
            disabled={disabled}
            className="min-w-[240px] rounded border border-gray-200 bg-gray-50 px-2 py-1 font-mono text-[11px] text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:outline-none disabled:opacity-50"
            placeholder={buildLedgerPath("1")}
            aria-label="Custom Ledger derivation path"
          />
        </>
      )}
      <button
        type="button"
        onClick={toggleCustomMode}
        disabled={disabled}
        className="rounded border border-gray-200 px-2 py-1 text-[11px] text-gray-600 hover:bg-gray-50 disabled:opacity-50"
      >
        {customMode ? "Use assigned path" : "Custom path"}
      </button>
    </div>
  );
}
