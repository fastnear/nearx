interface DualUnitInputProps {
  label: string;
  primaryLabel: string;
  secondaryLabel: string;
  primaryValue: string;
  secondaryValue: string;
  onPrimaryChange: (value: string) => void;
  onSecondaryChange: (value: string) => void;
  primaryPlaceholder?: string;
  secondaryPlaceholder?: string;
  error?: string | null;
}

const inputClass =
  "w-full rounded border border-gray-200 bg-gray-50 px-2.5 py-1.5 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:outline-none";

export default function DualUnitInput({
  label,
  primaryLabel,
  secondaryLabel,
  primaryValue,
  secondaryValue,
  onPrimaryChange,
  onSecondaryChange,
  primaryPlaceholder,
  secondaryPlaceholder,
  error,
}: DualUnitInputProps) {
  return (
    <div className="border-b border-gray-100 px-4 py-2.5 last:border-b-0">
      <div className="grid gap-3 sm:grid-cols-[7rem_minmax(0,1fr)]">
        <div className="pt-1 text-right text-sm text-gray-500">{label}</div>
        <div className="space-y-2">
          <div className="grid gap-2 sm:grid-cols-2">
            <label className="block">
              <span className="mb-1 block text-[11px] uppercase text-gray-500">
                {primaryLabel}
              </span>
              <input
                type="text"
                value={primaryValue}
                onChange={(event) => onPrimaryChange(event.target.value)}
                placeholder={primaryPlaceholder}
                className={inputClass}
              />
            </label>
            <label className="block">
              <span className="mb-1 block text-[11px] uppercase text-gray-500">
                {secondaryLabel}
              </span>
              <input
                type="text"
                value={secondaryValue}
                onChange={(event) => onSecondaryChange(event.target.value)}
                placeholder={secondaryPlaceholder}
                className={`${inputClass} font-mono`}
              />
            </label>
          </div>
          {error ? <div className="text-xs text-rose-600">{error}</div> : null}
        </div>
      </div>
    </div>
  );
}
