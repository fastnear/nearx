import type { ReactNode } from "react";
import FilterableCombobox from "./FilterableCombobox";

interface QuickSelectOption {
  value: string;
  label: string;
}

interface QuickSelectorConfig {
  label: string;
  value: string;
  options: QuickSelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  placeholder?: string;
  meta?: ReactNode;
  starredValues?: Set<string>;
  onToggleStar?: (value: string) => void;
}

interface SignerQuickSelectorsProps {
  account?: QuickSelectorConfig;
  keyOption?: QuickSelectorConfig;
}

function renderSelect(config: QuickSelectorConfig) {
  const {
    label,
    value,
    options,
    onChange,
    disabled = false,
    placeholder = "Not available",
    meta,
    starredValues,
    onToggleStar,
  } = config;
  const hasOptions = options.length > 0;
  const effectiveValue =
    hasOptions && options.some((option) => option.value === value) ? value : "";

  return (
    <div className="min-w-0">
      <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
        {label}
      </span>
      <FilterableCombobox
        label={label}
        value={effectiveValue}
        options={options}
        onChange={onChange}
        disabled={disabled || !hasOptions}
        placeholder={placeholder}
        starredValues={starredValues}
        onToggleStar={onToggleStar}
      />
      {meta ? <div className="mt-1.5 text-xs">{meta}</div> : null}
    </div>
  );
}

export default function SignerQuickSelectors({
  account,
  keyOption,
}: SignerQuickSelectorsProps) {
  if (!account && !keyOption) {
    return null;
  }

  return (
    <div className="grid gap-3 border-b border-gray-100 px-4 py-4">
      {account ? renderSelect(account) : null}
      {keyOption ? renderSelect(keyOption) : null}
    </div>
  );
}
