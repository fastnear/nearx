interface QuickSelectOption {
  value: string;
  label: string;
}

interface QuickSelectorAction {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  loading?: boolean;
  loadingLabel?: string;
}

interface QuickSelectorConfig {
  label: string;
  value: string;
  options: QuickSelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  placeholder?: string;
  action?: QuickSelectorAction;
}

interface SignerQuickSelectorsProps {
  account?: QuickSelectorConfig;
  keyOption?: QuickSelectorConfig;
  source?: QuickSelectorConfig;
}

const selectClass =
  "w-full min-w-0 rounded border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-900 focus:border-blue-500 focus:outline-none disabled:opacity-50";

function renderSelect(config: QuickSelectorConfig) {
  const {
    label,
    value,
    options,
    onChange,
    disabled = false,
    placeholder = "Not available",
    action,
  } = config;
  const hasOptions = options.length > 0;
  const effectiveValue =
    hasOptions && options.some((option) => option.value === value) ? value : "";

  return (
    <label className="block min-w-0 text-sm text-gray-700">
      <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
        {label}
      </span>
      <div className="flex items-center gap-1.5">
        <select
          value={effectiveValue}
          onChange={(event) => onChange(event.target.value)}
          disabled={disabled || !hasOptions}
          className={selectClass}
        >
          {!hasOptions ? <option value="">{placeholder}</option> : null}
          {hasOptions && !effectiveValue ? <option value="">{placeholder}</option> : null}
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        {action ? (
          <button
            type="button"
            onClick={action.onClick}
            disabled={action.disabled}
            className="shrink-0 whitespace-nowrap rounded border border-blue-300 bg-blue-50 px-2.5 py-2 text-xs font-medium text-blue-700 hover:bg-blue-100 disabled:opacity-50"
          >
            {action.loading ? action.loadingLabel ?? "Loading..." : action.label}
          </button>
        ) : null}
      </div>
    </label>
  );
}

export default function SignerQuickSelectors({
  account,
  keyOption,
  source,
}: SignerQuickSelectorsProps) {
  if (!account && !keyOption && !source) {
    return null;
  }

  return (
    <div className="grid gap-3 border-b border-gray-100 px-4 py-4 md:grid-cols-3">
      {account ? renderSelect(account) : null}
      {keyOption ? renderSelect(keyOption) : null}
      {source ? renderSelect(source) : null}
    </div>
  );
}
