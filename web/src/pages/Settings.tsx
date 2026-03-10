import { useState, useEffect, useCallback, useMemo } from "react";
import { KeyRound, Shield, Info, Usb, Loader2 } from "lucide-react";
import {
  getSigningCapabilitiesCached,
  listNearSigningAccounts,
  listNearSigningKeys,
  isTauriRuntime,
} from "../tauri/runtime";
import type {
  SigningCapabilities,
  SigningKeyEntry,
} from "../tauri/runtime";
import { networkId } from "../config";
import SigningKeyLabelEditor from "../components/SigningKeyLabelEditor";
import { usePreferences } from "../hooks/usePreferences";
import { secureStoreBackendLabel } from "../tauri/signingCapabilities";
import { signingKeyId } from "../lib/signerSourceSelection";
import { shortPublicKey } from "../lib/hardwareWalletDisplay";

const sectionClass = "rounded-lg border border-gray-200 bg-surface shadow-sm text-sm";
const sectionHeaderClass =
  "flex items-center gap-2 border-b border-gray-100 px-4 py-3 text-sm font-semibold text-gray-900";

function securityBadge(key: SigningKeyEntry) {
  const level = key.security_level ?? "basic";
  switch (level) {
    case "secure":
      return { label: "Secure", className: "border-emerald-200 text-emerald-700" };
    case "hardware":
      return { label: "Hardware", className: "border-blue-200 text-blue-700" };
    default:
      return { label: "Standard", className: "border-gray-200 text-gray-600" };
  }
}

interface AccountGroup {
  accountId: string;
  rows: Array<{ key: SigningKeyEntry; keyId: string; badge: ReturnType<typeof securityBadge> }>;
}

export default function Settings() {
  const { preferences, loading: prefsLoading, error: prefsError, updatePreference } = usePreferences();
  const [capabilities, setCapabilities] = useState<SigningCapabilities | null>(null);
  const [allKeys, setAllKeys] = useState<SigningKeyEntry[]>([]);
  const [keysLoading, setKeysLoading] = useState(true);
  const [keysError, setKeysError] = useState<string | null>(null);
  const [editingKeyId, setEditingKeyId] = useState<string | null>(null);

  useEffect(() => {
    void getSigningCapabilitiesCached().then((caps) => {
      if (caps) setCapabilities(caps);
    });
  }, []);

  const loadKeys = useCallback(async () => {
    if (!isTauriRuntime()) {
      setKeysLoading(false);
      return;
    }
    setKeysLoading(true);
    setKeysError(null);
    try {
      const accountResult = await listNearSigningAccounts({ network: networkId });
      const keyLists = await Promise.all(
        accountResult.accounts.map((account) =>
          listNearSigningKeys({ network: networkId, account_id: account.account_id }),
        ),
      );
      setAllKeys(keyLists.flatMap((result) => result.keys));
    } catch (err) {
      setKeysError(err instanceof Error ? err.message : String(err));
    } finally {
      setKeysLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadKeys();
  }, [loadKeys]);

  const patchKeyLabel = useCallback(
    (accountId: string, publicKey: string, label: string | null | undefined) => {
      setAllKeys((prev) =>
        prev.map((k) =>
          k.account_id === accountId && k.public_key === publicKey
            ? { ...k, label: label ?? undefined }
            : k,
        ),
      );
    },
    [],
  );

  const accountGroups = useMemo(() => {
    const groups = new Map<string, AccountGroup>();
    for (const key of allKeys) {
      let group = groups.get(key.account_id);
      if (!group) {
        group = { accountId: key.account_id, rows: [] };
        groups.set(key.account_id, group);
      }
      group.rows.push({ key, keyId: signingKeyId(key), badge: securityBadge(key) });
    }
    return [...groups.values()];
  }, [allKeys]);

  if (!isTauriRuntime()) {
    return (
      <div className="mx-auto max-w-3xl">
        <div className="rounded-lg border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-700">
          <Info className="mr-1.5 inline size-3.5 align-text-bottom" />
          Settings are only available in the NEARx desktop app.
        </div>
      </div>
    );
  }

  const initialLoading = keysLoading && allKeys.length === 0;

  if (initialLoading) {
    return (
      <div className="mx-auto max-w-3xl space-y-6">
        <h1 className="mb-4 text-2xl font-bold tracking-tight text-gray-900">Settings</h1>
        <div className="flex items-center justify-center py-16 text-sm text-gray-500">
          <Loader2 className="mr-2 size-5 animate-spin text-gray-400" />
          Loading settings…
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <h1 className="mb-4 text-2xl font-bold tracking-tight text-gray-900">Settings</h1>

      {/* Security */}
      <div className={sectionClass}>
        <div className={sectionHeaderClass}>
          <Shield className="size-4" />
          Security
        </div>
        <div className="px-4 py-4 space-y-4">
          <label className="flex items-start gap-3">
            <input
              type="checkbox"
              checked={preferences.always_prompt_user_presence}
              disabled={prefsLoading || !capabilities?.supports_user_presence}
              onChange={(e) =>
                void updatePreference("always_prompt_user_presence", e.target.checked)
              }
              className="mt-0.5 size-4 rounded border-gray-300 text-blue-600 focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
            />
            <div className="min-w-0">
              <div className="text-sm font-medium text-gray-900">
                Always require fingerprint
              </div>
              <div className="text-sm text-gray-500">
                Require biometric verification before every signing operation,
                including file-based credentials.
              </div>
              {capabilities && !capabilities.supports_user_presence && (
                <div className="mt-1 flex items-center gap-1 text-xs text-amber-600">
                  <Info className="size-3" />
                  Biometric verification is not available on this platform.
                </div>
              )}
            </div>
          </label>
          {prefsError && (
            <div className="rounded-lg border border-rose-200 bg-rose-50 px-4 py-3 text-sm text-rose-700">
              {prefsError}
            </div>
          )}
        </div>
      </div>

      {/* Signing Keys */}
      <div className={sectionClass}>
        <div className={sectionHeaderClass}>
          <KeyRound className="size-4" />
          Signing Keys
        </div>
        {keysError && (
          <div className="rounded-lg border border-rose-200 bg-rose-50 mx-4 mt-3 px-4 py-3 text-sm text-rose-700">
            {keysError}
          </div>
        )}
        {allKeys.length === 0 && !keysError && (
          <div className="px-4 py-4 text-sm text-gray-500">
            No signing keys found. Import credentials from the NEAR CLI or
            connect a hardware wallet to get started.
          </div>
        )}
        {accountGroups.map((group) => (
          <div key={group.accountId}>
            <div className="border-b border-gray-100 bg-gray-50/50 px-4 py-2">
              <span className="font-mono text-xs font-medium text-gray-700">{group.accountId}</span>
              <span className="ml-2 text-xs text-gray-400">
                {group.rows.length} {group.rows.length === 1 ? "key" : "keys"}
              </span>
            </div>
            {group.rows.map(({ key, keyId, badge }) => (
              <div
                key={keyId}
                className="border-b border-gray-100 px-4 py-3 last:border-b-0"
              >
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-gray-900">
                      {key.label ?? <span className="font-normal text-gray-400">No label</span>}
                    </div>
                    <div className="mt-0.5 font-mono text-xs text-gray-500">
                      {shortPublicKey(key.public_key)}
                    </div>
                    <div className="mt-1 flex flex-wrap gap-1.5">
                      <span
                        className={`inline-flex rounded-full border px-2 py-0.5 text-[11px] ${badge.className}`}
                      >
                        {badge.label}
                      </span>
                      {key.stale && (
                        <span className="inline-flex rounded-full border border-gray-200 px-2 py-0.5 text-[11px] text-gray-400">
                          Stale
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    <button
                      type="button"
                      onClick={() =>
                        setEditingKeyId((prev) => (prev === keyId ? null : keyId))
                      }
                      className="rounded-md border border-gray-300 px-2.5 py-1.5 text-xs text-gray-600 transition-colors hover:bg-gray-50 focus:ring-2 focus:ring-blue-500/20 focus:outline-none"
                    >
                      {editingKeyId === keyId ? "Close" : "Edit label"}
                    </button>
                  </div>
                </div>
                {editingKeyId === keyId && (
                  <div className="mt-3 border-t border-gray-100 pt-3">
                    <SigningKeyLabelEditor
                      entry={key}
                      network={networkId}
                      onSaved={(result) => {
                        if (result) {
                          patchKeyLabel(result.account_id, result.public_key, result.label);
                        }
                      }}
                    />
                  </div>
                )}
              </div>
            ))}
          </div>
        ))}
        <div className="flex items-center gap-1.5 border-t border-gray-100 px-4 py-3 text-sm text-gray-500">
          <Usb className="size-3.5" />
          Connect a Ledger device from the{" "}
          <a href="#/staking" className="text-blue-600 hover:underline">
            Staking
          </a>{" "}
          or{" "}
          <a href="#/sign" className="text-blue-600 hover:underline">
            Sign Transaction
          </a>{" "}
          page.
        </div>
      </div>

      {/* Platform Info */}
      <div className={sectionClass}>
        <div className={sectionHeaderClass}>
          <Info className="size-4" />
          Platform
        </div>
        <dl className="grid gap-x-8 gap-y-3 px-4 py-4 sm:grid-cols-2">
          {capabilities && (
            <>
              <div>
                <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
                  Platform
                </dt>
                <dd className="mt-0.5 text-sm text-gray-900">
                  {capabilities.platform}
                </dd>
              </div>
              <div>
                <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
                  Secure Store
                </dt>
                <dd className="mt-0.5 text-sm text-gray-900">
                  {secureStoreBackendLabel(capabilities) ?? capabilities.secure_store_backend}
                </dd>
              </div>
              <div>
                <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
                  Biometric Support
                </dt>
                <dd className="mt-0.5 text-sm text-gray-900">
                  {capabilities.supports_user_presence ? "Available" : "Not available"}
                </dd>
              </div>
              <div>
                <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
                  Hardware Wallet
                </dt>
                <dd className="mt-0.5 text-sm text-gray-900">
                  {capabilities.supports_hardware_wallet_connect
                    ? "Connect supported"
                    : "Not available"}
                  {capabilities.supports_hardware_wallet_sign && " + Sign supported"}
                </dd>
              </div>
              <div>
                <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
                  Transport
                </dt>
                <dd className="mt-0.5 text-sm text-gray-900">
                  {capabilities.transport}
                </dd>
              </div>
              <div>
                <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
                  Network
                </dt>
                <dd className="mt-0.5 text-sm text-gray-900">{networkId}</dd>
              </div>
            </>
          )}
          {!capabilities && (
            <div className="col-span-2 text-sm text-gray-500">
              Loading capabilities...
            </div>
          )}
        </dl>
      </div>
    </div>
  );
}
