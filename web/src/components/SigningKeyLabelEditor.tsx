import { useEffect, useId, useMemo, useState } from "react";
import {
  isTauriRuntime,
  setSigningKeyLabel,
} from "../tauri/runtime";
import type {
  CredentialSource,
  SetSigningKeyLabelResult,
  SigningKeyEntry,
} from "../tauri/runtime";

const GENERIC_LABEL_SUGGESTIONS = ["Primary", "Backup"];

const SOURCE_LABEL_SUGGESTIONS: Partial<Record<CredentialSource, string>> = {
  legacy_file: "File system",
  near_cli_secure: "OS secrets",
  nearxd_keychain: "Keychain",
  hardware_wallet: "Ledger",
};

interface SigningKeyLabelEditorProps {
  entry: SigningKeyEntry | null;
  network: string;
  disabled?: boolean;
  onSaved?: (result: SetSigningKeyLabelResult) => void;
}

function buildLabelSuggestions(entry: SigningKeyEntry | null): string[] {
  const suggestions = [...GENERIC_LABEL_SUGGESTIONS];
  if (!entry) {
    return suggestions;
  }
  for (const source of entry.available_sources) {
    const suggested = SOURCE_LABEL_SUGGESTIONS[source];
    if (suggested && !suggestions.includes(suggested)) {
      suggestions.push(suggested);
    }
  }
  return suggestions;
}

export default function SigningKeyLabelEditor({
  entry,
  network,
  disabled = false,
  onSaved,
}: SigningKeyLabelEditorProps) {
  const datalistId = useId();
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const suggestions = useMemo(() => buildLabelSuggestions(entry), [entry]);

  useEffect(() => {
    setDraft(entry?.label ?? "");
    setMessage(null);
    setError(null);
  }, [entry?.account_id, entry?.public_key, entry?.label]);

  if (!entry || !isTauriRuntime()) {
    return null;
  }

  const currentEntry = entry;

  const currentLabel = currentEntry.label?.trim() ?? "";
  const nextLabel = draft.trim();
  const dirty = nextLabel !== currentLabel;

  async function saveLabel(labelOverride?: string | null) {
    const label = (labelOverride ?? draft).trim();
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const result = await setSigningKeyLabel({
        network,
        account_id: currentEntry.account_id,
        public_key: currentEntry.public_key,
        label: label || null,
      });
      if (labelOverride !== undefined) {
        setDraft(label);
      }
      if (onSaved) {
        onSaved(result);
      }
      setMessage(label ? "Label saved." : "Label cleared.");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-3">
      <div>
        <div className="text-xs uppercase text-gray-500">Key Label</div>
        <div className="text-sm text-gray-500">
          Optional. Use a short label to tell signer keys apart across pages.
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="text"
          list={datalistId}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          disabled={disabled || saving}
          className="min-w-[220px] flex-1 rounded-md border border-gray-300 bg-surface px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20 focus:outline-none disabled:opacity-50"
          placeholder="Primary staking key"
          maxLength={64}
          aria-label="Signer key label"
        />
        <datalist id={datalistId}>
          {suggestions.map((suggestion) => (
            <option key={suggestion} value={suggestion} />
          ))}
        </datalist>
        <button
          type="button"
          onClick={() => void saveLabel()}
          disabled={disabled || saving || !dirty}
          className="rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:outline-none disabled:opacity-50"
        >
          {saving ? "Saving..." : "Save Label"}
        </button>
        <button
          type="button"
          onClick={() => void saveLabel("")}
          disabled={disabled || saving || !currentLabel}
          className="rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-600 transition-colors hover:bg-gray-50 focus:ring-2 focus:ring-blue-500/20 focus:outline-none disabled:opacity-50"
        >
          Clear
        </button>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {suggestions.map((suggestion) => (
          <button
            key={suggestion}
            type="button"
            onClick={() => setDraft(suggestion)}
            disabled={disabled || saving}
            className={`rounded-md border px-2.5 py-1 text-xs ${
              draft.trim() === suggestion
                ? "border-blue-300 bg-blue-50 text-blue-700"
                : "border-gray-300 text-gray-600 hover:bg-gray-50"
            } disabled:opacity-50`}
          >
            {suggestion}
          </button>
        ))}
      </div>
      {message && <div className="text-xs text-green-600">{message}</div>}
      {error && <div className="text-xs text-amber-700">{error}</div>}
    </div>
  );
}
