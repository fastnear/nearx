import { Star, KeyRound } from "lucide-react";
import type { CredentialSource, SigningCapabilities, SigningKeyEntry } from "../tauri/runtime";
import { ledgerPathBadge } from "../lib/hardwareWalletDisplay";
import { credentialSourceLabel } from "../tauri/signingCapabilities";

export interface AccountPickerKeyBadge {
  text: string;
  tone?: "info" | "warn" | "danger";
  title?: string;
}

interface AccountPickerProps {
  keys: SigningKeyEntry[];
  selectedAccountId: string;
  selectedPublicKey: string;
  onSelect: (entry: SigningKeyEntry) => void;
  keyBadge?: (entry: SigningKeyEntry) => AccountPickerKeyBadge | null;
  signingCapabilities?: SigningCapabilities | null;
  isStarred: (id: string) => boolean;
  onToggleStar: (id: string) => void;
}

function keyId(key: SigningKeyEntry): string {
  return `${key.account_id}:${key.public_key}`;
}

function permissionLabel(key: SigningKeyEntry): string {
  switch (key.permission.kind) {
    case "full_access":
      return "Full Access";
    case "function_call":
      return "Function Call";
    default:
      return "Unknown";
  }
}

function labelBadgeText(key: SigningKeyEntry): string | null {
  const label = key.label?.trim();
  return label ? label : null;
}

function primaryUsableSource(key: SigningKeyEntry): CredentialSource | null {
  return key.preferred_source ?? key.available_sources[0] ?? null;
}

export default function AccountPicker({
  keys,
  selectedAccountId,
  selectedPublicKey,
  onSelect,
  keyBadge,
  signingCapabilities,
  isStarred,
  onToggleStar,
}: AccountPickerProps) {
  const grouped = new Map<string, SigningKeyEntry[]>();
  for (const key of keys) {
    if (!grouped.has(key.account_id)) grouped.set(key.account_id, []);
    grouped.get(key.account_id)!.push(key);
  }
  const accounts = [...grouped.keys()].sort((a, b) => {
    const aS = isStarred(a);
    const bS = isStarred(b);
    if (aS !== bS) return aS ? -1 : 1;
    return a.localeCompare(b);
  });

  return (
    <div className="max-h-64 overflow-y-auto">
      {accounts.map((accountId) => {
        const accountKeys = grouped
          .get(accountId)!
          .slice()
          .sort((a, b) => {
            const af = a.permission.kind === "full_access" ? 0 : 1;
            const bf = b.permission.kind === "full_access" ? 0 : 1;
            if (af !== bf) return af - bf;
            return a.public_key.localeCompare(b.public_key);
          });
        const starred = isStarred(accountId);
        return (
          <div key={accountId} className="border-b border-gray-100 last:border-b-0">
            <div className="flex items-center gap-2 bg-gray-50 px-4 py-2.5">
              <button
                type="button"
                onClick={() => onToggleStar(accountId)}
                className="shrink-0 rounded p-0.5 hover:bg-gray-200"
                title={starred ? "Unstar" : "Star"}
              >
                <Star
                  size={13}
                  className={
                    starred ? "fill-yellow-400 text-yellow-400" : "text-gray-300"
                  }
                />
              </button>
              <span className="truncate font-mono text-base text-gray-800" title={accountId}>
                {accountId}
              </span>
            </div>

            {accountKeys.map((key) => {
              const selected =
                key.account_id === selectedAccountId && key.public_key === selectedPublicKey;
              const badge = keyBadge?.(key) ?? null;
              const kid = keyId(key);
              const readySource = primaryUsableSource(key);
              return (
                <div
                  key={kid}
                  className={`flex cursor-pointer items-center gap-3 px-4 py-3 ${
                    selected ? "bg-blue-600/10" : "hover:bg-gray-50"
                  }`}
                  onClick={() => onSelect(key)}
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-mono text-sm text-gray-700" title={key.public_key}>
                      {key.public_key}
                    </div>
                    <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs">
                      {labelBadgeText(key) && (
                        <span
                          title={labelBadgeText(key) ?? undefined}
                          className="max-w-[14rem] truncate rounded bg-slate-100 px-1.5 py-0.5 text-slate-700"
                        >
                          {labelBadgeText(key)}
                        </span>
                      )}
                      <span className="rounded bg-gray-100 px-1.5 py-0.5 text-gray-600">
                        {key.curve_type}
                      </span>
                      <span
                        className={`rounded px-1.5 py-0.5 ${
                          key.permission.kind === "full_access"
                            ? "bg-green-100 text-green-700"
                            : key.permission.kind === "function_call"
                              ? "bg-amber-100 text-amber-700"
                              : "bg-gray-100 text-gray-600"
                        }`}
                      >
                        {permissionLabel(key)}
                      </span>
                      {key.available_sources.map((source) => (
                        <span key={source} className="rounded bg-blue-50 px-1.5 py-0.5 text-blue-700">
                          {credentialSourceLabel(source, signingCapabilities)}
                        </span>
                      ))}
                      {key.hardware_wallet?.derivation_path && (
                        <span
                          title={`Ledger path ${key.hardware_wallet.derivation_path}`}
                          className="rounded bg-indigo-50 px-1.5 py-0.5 font-mono text-indigo-700"
                        >
                          {ledgerPathBadge(key.hardware_wallet.derivation_path)}
                        </span>
                      )}
                      {key.stale === true && (
                        <span
                          title="Indexed key metadata is old and may no longer reflect current local state."
                          className="rounded bg-amber-100 px-1.5 py-0.5 text-amber-800"
                        >
                          Stale
                        </span>
                      )}
                      {badge && (
                        <span
                          title={badge.title ?? badge.text}
                          className={`rounded px-1.5 py-0.5 ${
                            badge.tone === "warn"
                              ? "bg-amber-100 text-amber-800"
                              : badge.tone === "danger"
                                ? "bg-rose-100 text-rose-700"
                                : "bg-slate-100 text-slate-700"
                          }`}
                        >
                          {badge.text}
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="shrink-0 text-right">
                    {key.in_nearxd_keychain ? (
                      <span className="flex items-center justify-end gap-1 text-sm text-green-600">
                        <KeyRound size={11} />
                        Ready
                      </span>
                    ) : readySource ? (
                      <span className="text-sm text-gray-600">
                        {credentialSourceLabel(readySource, signingCapabilities)}
                      </span>
                    ) : (
                      <span className="text-sm text-gray-400">Unavailable</span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}
