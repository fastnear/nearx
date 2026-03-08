import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Download, KeyRound, ChevronUp, Plus, Settings2, Trash2, Usb } from "lucide-react";
import {
  importNearSigningKeys,
  listStakingWatchlist,
  addStakingWatchlistAccount,
  removeStakingWatchlistAccount,
  connectHardwareWallet,
  getSigningCapabilitiesCached,
  isTauriRuntime,
  reprotectNearSigningKey,
} from "../tauri/runtime";
import type {
  ConnectHardwareWalletResult,
  CredentialSource,
  SigningCapabilities,
  SigningKeyEntry,
  StakingWatchlistEntry,
} from "../tauri/runtime";
import { getAccountFull } from "../api/fastnearApi";
import {
  getValidators,
  getPoolBalancesBatch,
  executeStakingAction,
  StakingBroadcastError,
} from "../api/staking";
import type { ValidatorInfo, PoolBalance } from "../api/staking";
import { networkId } from "../config";
import NearAmount from "../components/NearAmount";
import AccountId from "../components/AccountId";
import AccountPicker from "../components/AccountPicker";
import LedgerConnectionPanel from "../components/LedgerConnectionPanel";
import SigningKeyLabelEditor from "../components/SigningKeyLabelEditor";
import ManageSignerPanel from "../components/ManageSignerPanel";
import SignerSummaryCard from "../components/SignerSummaryCard";
import SignerQuickSelectors from "../components/SignerQuickSelectors";
import { useAccountPrefs } from "../hooks/useAccountPrefs";
import useSignerSelection from "../hooks/useSignerSelection";
import { resolveSignerSummaryStatus } from "../lib/signerSummaryStatus";
import {
  keychainUpgradeFallbackMessage,
  isUpgradeEligible,
  resolveSourceUpgradeKind,
  upgradeButtonLabel,
  upgradeLoadingLabel,
} from "../lib/sourceUpgrade";
import type { BroadcastSummary } from "../lib/broadcastSummary";
import { summarizeBroadcastResult } from "../lib/broadcastSummary";
import {
  availableImportSources,
  buildImportParams,
  credentialSourceLabel,
  DEFAULT_LEDGER_DERIVATION_PATH,
  secureStorageLabel,
  secureStoreBackendLabel,
} from "../tauri/signingCapabilities";
import {
  permissionLabel,
  signingAccountOptionLabel,
  signingKeyOptionLabel,
  stakingWatchlistMeta,
  stakingWatchlistOptionLabel,
  shortPublicKey,
} from "../lib/hardwareWalletDisplay";
import {
  fallbackLocalSource,
  preferSigningKey,
  resolveCredentialSource,
  signerSourceLabel,
  signerSourceOptions as signerSourceOptionsForKey,
  signingKeyId,
} from "../lib/signerSourceSelection";
import {
  ledgerHardwareErrorMessage,
  snapshotFromConnectResult,
  type ConnectedLedgerSnapshot,
  type SignerModalTab,
} from "../lib/ledgerConnectionUi";

type ActionKind = "deposit_and_stake" | "unstake" | "unstake_all" | "withdraw" | "withdraw_all";

interface ExpandedRow {
  poolId: string;
  action: ActionKind;
}

const NEAR_DECIMALS = 24;
const DEV_LOG = import.meta.env.DEV;

function devLog(...args: unknown[]) {
  if (DEV_LOG) console.log(...args);
}

function devWarn(...args: unknown[]) {
  if (DEV_LOG) console.warn(...args);
}

function devError(...args: unknown[]) {
  if (DEV_LOG) console.error(...args);
}

function nearToYocto(near: string): string {
  const parts = near.split(".");
  const whole = parts[0] || "0";
  const frac = (parts[1] || "").padEnd(NEAR_DECIMALS, "0").slice(0, NEAR_DECIMALS);
  const raw = whole + frac;
  return raw.replace(/^0+/, "") || "0";
}

function yoctoToNear(yocto: string): string {
  const padded = yocto.padStart(NEAR_DECIMALS + 1, "0");
  const whole = padded.slice(0, padded.length - NEAR_DECIMALS);
  const frac = padded.slice(padded.length - NEAR_DECIMALS).replace(/0+$/, "");
  return frac ? `${whole}.${frac}` : whole;
}

export default function Staking() {
  const [searchParams] = useSearchParams();
  const initialAccount = searchParams.get("account") || undefined;

  const [watchlist, setWatchlist] = useState<StakingWatchlistEntry[]>([]);
  const [watchlistLoading, setWatchlistLoading] = useState(isTauriRuntime());
  const [watchlistInput, setWatchlistInput] = useState("");
  const {
    lastAccountId,
    lastPublicKey,
    lastCredentialSource,
    getLastSource,
    toggleStar,
    setLastAccount,
    setLastKey,
    setLastSource,
    sortAccounts,
    isStarred,
  } = useAccountPrefs("staking");
  const [showAllKeys, setShowAllKeys] = useState(false);
  const [importSources, setImportSources] = useState<CredentialSource[]>([
    "legacy_file",
    "near_cli_secure",
  ]);
  const [signingCapabilities, setSigningCapabilities] = useState<SigningCapabilities | null>(null);
  const [ledgerConnecting, setLedgerConnecting] = useState(false);
  const [showLedgerPanel, setShowLedgerPanel] = useState(false);
  const [ledgerTiedToSelectedAccount, setLedgerTiedToSelectedAccount] = useState(false);
  const [ledgerDerivationPath, setLedgerDerivationPath] = useState(
    DEFAULT_LEDGER_DERIVATION_PATH,
  );
  const [ledgerConnection, setLedgerConnection] =
    useState<ConnectHardwareWalletResult | null>(null);
  const [hardwareMsg, setHardwareMsg] = useState<string | null>(null);
  const [hardwareError, setHardwareError] = useState<string | null>(null);

  const [poolBalances, setPoolBalances] = useState<PoolBalance[]>([]);
  const [validators, setValidators] = useState<ValidatorInfo[]>([]);
  const [loadProgress, setLoadProgress] = useState<{ done: number; total: number } | null>(null);
  const [dataLoading, setDataLoading] = useState(false);
  const [validatorsLoading, setValidatorsLoading] = useState(false);

  const [expanded, setExpanded] = useState<ExpandedRow | null>(null);
  const [actionAmount, setActionAmount] = useState("");
  const [actionLoading, setActionLoading] = useState(false);
  const [actionResult, setActionResult] = useState<BroadcastSummary | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const [importing, setImporting] = useState(false);
  const [importingKeyId, setImportingKeyId] = useState<string | undefined>();
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showManageSigner, setShowManageSigner] = useState(false);
  const [activeSignerTab, setActiveSignerTab] = useState<SignerModalTab>("account_key");
  const [connectedLedgerSnapshot, setConnectedLedgerSnapshot] =
    useState<ConnectedLedgerSnapshot | null>(null);
  const [ledgerNotice, setLedgerNotice] = useState<string | null>(null);

  const {
    accountsLoading,
    allSignableKeys,
    keys,
    load: loadAccounts,
    loadError,
    selectAccount: selectSignerAccount,
    selectKey: selectSignerKey,
    selectedAccountId: activeAccountId,
    selectedCredentialSource,
    selectedEntry,
    selectedKeyId,
    selectedPublicKey: selectedKey,
    setCredentialSource,
    setSelection: setSignerSelection,
    signingAccounts,
  } = useSignerSelection({
    network: networkId,
    initialAccountId: initialAccount ?? lastAccountId ?? "",
    lastAccountId,
    lastPublicKey,
    lastCredentialSource,
    getLastSource,
    sortAccounts,
    setLastAccount,
    setLastKey,
    setLastSource,
    autoLoad: false,
  });

  const secureStoreName = secureStoreBackendLabel(signingCapabilities);
  const storageLabel = secureStorageLabel();
  const canImportLegacy = signingCapabilities?.supports_legacy_import ?? true;
  const canImportNearCli = signingCapabilities?.supports_near_cli_secure ?? true;
  const canPersistSecureStore =
    signingCapabilities?.supports_secure_store_persistence ?? true;
  const canConnectLedger =
    signingCapabilities?.supports_hardware_wallet_connect ?? true;

  const activeAccountIdRef = useRef(activeAccountId);
  const lastAccountIdRef = useRef(lastAccountId);
  const loadAccountsRef = useRef(loadAccounts);

  useEffect(() => {
    activeAccountIdRef.current = activeAccountId;
  }, [activeAccountId]);

  useEffect(() => {
    lastAccountIdRef.current = lastAccountId;
  }, [lastAccountId]);

  useEffect(() => {
    loadAccountsRef.current = loadAccounts;
  }, [loadAccounts]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    getSigningCapabilitiesCached()
      .then((caps) => {
        if (cancelled) return;
        setSigningCapabilities(caps);
        const allowed = availableImportSources(caps);
        setImportSources((prev) => {
          const next = prev.filter((source) => allowed.includes(source));
          return next.length > 0 ? next : allowed;
        });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const loadWatchlist = useCallback(() => {
    if (!isTauriRuntime()) {
      const fallback =
        activeAccountIdRef.current ||
        initialAccount ||
        lastAccountIdRef.current ||
        "";
      void loadAccountsRef.current(fallback || undefined);
      return;
    }
    setWatchlistLoading(true);
    listStakingWatchlist({ network: networkId })
      .then((result) => {
        setWatchlist(result.entries);
        const fallback =
          activeAccountIdRef.current ||
          initialAccount ||
          lastAccountIdRef.current ||
          result.entries[0]?.account_id ||
          "";
        void loadAccountsRef.current(fallback || undefined);
      })
      .catch((err) => {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
      })
      .finally(() => setWatchlistLoading(false));
  }, [initialAccount]);

  useEffect(() => {
    loadWatchlist();
  }, [loadWatchlist]);

  useEffect(() => {
    if (!loadError) {
      return;
    }
    setError(loadError);
  }, [loadError]);

  const visibleKeys = useMemo(() => {
    if (showAllKeys) return keys;
    return keys.filter((k) => k.permission.kind === "full_access");
  }, [keys, showAllKeys]);
  const selectedLedgerSnapshot = useMemo(() => {
    if (!connectedLedgerSnapshot) {
      return null;
    }
    const matchesSelectedKey =
      activeAccountId.trim() === connectedLedgerSnapshot.accountId &&
      selectedKey.trim() === connectedLedgerSnapshot.publicKey;
    const matchesHardwareSelection =
      selectedCredentialSource === "hardware_wallet" &&
      activeAccountId.trim() === connectedLedgerSnapshot.accountId;
    return matchesSelectedKey || matchesHardwareSelection ? connectedLedgerSnapshot : null;
  }, [
    activeAccountId,
    connectedLedgerSnapshot,
    selectedCredentialSource,
    selectedKey,
  ]);
  const effectiveCredentialSource = useMemo(
    () => {
      if (selectedEntry) {
        return resolveCredentialSource(selectedEntry, selectedCredentialSource);
      }
      if (selectedLedgerSnapshot && selectedCredentialSource === "hardware_wallet") {
        return "hardware_wallet";
      }
      return selectedCredentialSource;
    },
    [selectedCredentialSource, selectedEntry, selectedLedgerSnapshot],
  );

  const hasImportableKeys = useMemo(
    () => keys.some((k) => k.importable),
    [keys],
  );
  const allSignableKeyOptions = useMemo(
    () =>
      allSignableKeys.map((key) => ({
        id: signingKeyId(key),
        label: signingKeyOptionLabel(key),
      })),
    [allSignableKeys],
  );

  const selectedPermissionKind = useMemo(
    () => selectedEntry?.permission.kind ?? selectedLedgerSnapshot?.permission.kind ?? null,
    [selectedEntry, selectedLedgerSnapshot],
  );
  const selectedKeychainImportRequired = useMemo(
    () =>
      effectiveCredentialSource === "nearxd_keychain" &&
      Boolean(selectedEntry?.nearxd_keychain_import_required),
    [effectiveCredentialSource, selectedEntry],
  );
  const stakingActionReady = useMemo(
    () =>
      Boolean(
        (selectedEntry || selectedLedgerSnapshot) &&
        activeAccountId &&
        effectiveCredentialSource &&
        selectedPermissionKind === "full_access" &&
        !selectedKeychainImportRequired,
      ),
    [
      activeAccountId,
      effectiveCredentialSource,
      selectedEntry,
      selectedKeychainImportRequired,
      selectedLedgerSnapshot,
      selectedPermissionKind,
    ],
  );
  const selectedSourceLabel = useMemo(() => {
    if (effectiveCredentialSource) {
      return signerSourceLabel(
        effectiveCredentialSource,
        selectedEntry,
        signingCapabilities,
      );
    }
    return "Not selected";
  }, [effectiveCredentialSource, selectedEntry, signingCapabilities]);
  const signerSourceOptions = useMemo(() => {
    if (selectedEntry) {
      return signerSourceOptionsForKey(selectedEntry, signingCapabilities);
    }
    if (selectedLedgerSnapshot && effectiveCredentialSource === "hardware_wallet") {
      return [
        {
          value: "hardware_wallet",
          label: credentialSourceLabel("hardware_wallet", signingCapabilities),
        },
      ];
    }
    return [];
  }, [effectiveCredentialSource, selectedEntry, selectedLedgerSnapshot, signingCapabilities]);
  const selectedWeakSourceWarning = useMemo(() => {
    if (
      !selectedEntry ||
      signingCapabilities?.platform !== "macos" ||
      !selectedEntry.available_sources.includes("nearxd_keychain")
    ) {
      return null;
    }
    if (
      effectiveCredentialSource === "legacy_file" ||
      effectiveCredentialSource === "near_cli_secure"
    ) {
      if (selectedEntry.nearxd_keychain_import_required) {
        return `This key has a Keychain copy, but it is not fingerprint-protected yet. NEARx is using ${credentialSourceLabel(
          effectiveCredentialSource,
          signingCapabilities,
        )} instead.`;
      }
      return `Fingerprint verification is not used when signing from ${credentialSourceLabel(
        effectiveCredentialSource,
        signingCapabilities,
      )}.`;
    }
    return null;
  }, [effectiveCredentialSource, selectedEntry, signingCapabilities]);
  const selectedWeakSourceLabel = useMemo(() => {
    if (
      effectiveCredentialSource === "legacy_file" ||
      effectiveCredentialSource === "near_cli_secure"
    ) {
      return `Using ${credentialSourceLabel(
        effectiveCredentialSource,
        signingCapabilities,
      )}`;
    }
    return "Fingerprint off";
  }, [effectiveCredentialSource, signingCapabilities]);
  const sourceUpgradeKind = useMemo(
    () =>
      resolveSourceUpgradeKind(
        selectedEntry,
        effectiveCredentialSource,
        canPersistSecureStore,
      ),
    [canPersistSecureStore, effectiveCredentialSource, selectedEntry],
  );
  const selectedLedgerPath = useMemo(
    () =>
      selectedEntry?.hardware_wallet?.derivation_path ??
      selectedLedgerSnapshot?.derivationPath ??
      ledgerConnection?.derivation_path ??
      null,
    [
      ledgerConnection?.derivation_path,
      selectedEntry?.hardware_wallet?.derivation_path,
      selectedLedgerSnapshot?.derivationPath,
    ],
  );
  const resetSignerUi = useCallback(() => {
    setExpanded(null);
    setActionAmount("");
    setActionError(null);
    setActionResult(null);
    setHardwareError(null);
    setLedgerNotice(null);
  }, []);

  useEffect(() => {
    if (showAllKeys) return;
    if (!selectedEntry) return;
    if (selectedEntry.permission.kind === "full_access") return;
    const replacement =
      preferSigningKey(keys, {
        accountId: selectedEntry.account_id,
        publicKey: lastPublicKey,
      }) ?? preferSigningKey(keys);
    if (replacement) {
      selectSignerKey(replacement);
    }
  }, [keys, lastPublicKey, selectSignerKey, selectedEntry, showAllKeys]);

  useEffect(() => {
    if (!selectedEntry) {
      if (selectedLedgerSnapshot && selectedCredentialSource === "hardware_wallet") {
        return;
      }
      if (!selectedKey) {
        setCredentialSource(null, false);
      }
      return;
    }
    if (
      selectedCredentialSource &&
      selectedEntry.available_sources.includes(selectedCredentialSource) &&
      resolveCredentialSource(selectedEntry, selectedCredentialSource) ===
        selectedCredentialSource
    ) {
      return;
    }
    setCredentialSource(
      resolveCredentialSource(selectedEntry, selectedCredentialSource),
    );
  }, [selectedCredentialSource, selectedEntry, selectedKey, selectedLedgerSnapshot, setCredentialSource]);

  const ensureWatchlistAccount = useCallback(
    async (
      accountId: string,
      options?: {
        source?: StakingWatchlistEntry["source"];
        wallet_type?: ConnectHardwareWalletResult["wallet_type"];
        public_key?: string;
        derivation_path?: string;
      },
    ) => {
      const trimmed = accountId.trim();
      if (!trimmed || !isTauriRuntime()) {
        return;
      }
      if (
        watchlist.some(
          (entry) =>
            entry.account_id === trimmed &&
            (!options?.source || entry.source === options.source),
        )
      ) {
        return;
      }
      try {
        const result = await addStakingWatchlistAccount({
          network: networkId,
          account_id: trimmed,
          source: options?.source ?? "manual",
          wallet_type: options?.wallet_type,
          public_key: options?.public_key,
          derivation_path: options?.derivation_path,
        });
        setWatchlist(result.entries);
      } catch (err) {
        devWarn("[Staking] failed to seed watchlist account:", err);
      }
    },
    [watchlist],
  );

  const handleSelectKey = useCallback(
    (entry: SigningKeyEntry) => {
      resetSignerUi();
      selectSignerKey(entry);
    },
    [resetSignerUi, selectSignerKey],
  );

  const handleActiveAccountChange = useCallback(
    async (nextAccountId: string) => {
      const trimmed = nextAccountId.trim();
      if (!trimmed) {
        return;
      }
      resetSignerUi();
      setError(null);
      await ensureWatchlistAccount(trimmed, { source: "manual" });
      await selectSignerAccount(trimmed);
    },
    [ensureWatchlistAccount, resetSignerUi, selectSignerAccount],
  );

  useEffect(() => {
    if (!activeAccountId || watchlistLoading) {
      return;
    }
    if (watchlist.some((entry) => entry.account_id === activeAccountId)) {
      return;
    }
    void ensureWatchlistAccount(activeAccountId, { source: "manual" });
  }, [activeAccountId, ensureWatchlistAccount, watchlist, watchlistLoading]);

  // Fetch pool balances when account changes
  useEffect(() => {
    if (!activeAccountId) return;
    let cancelled = false;
    setDataLoading(true);
    setPoolBalances([]);
    setError(null);

    getAccountFull(activeAccountId)
      .then((resp) => {
        if (cancelled || !resp) {
          setDataLoading(false);
          return;
        }
        const poolIds = resp.pools.map((p) => p.pool_id);
        if (poolIds.length === 0) {
          setDataLoading(false);
          return;
        }
        setLoadProgress({ done: 0, total: poolIds.length });
        return getPoolBalancesBatch(poolIds, activeAccountId, (done) => {
          if (!cancelled) setLoadProgress({ done, total: poolIds.length });
        }).then((balances) => {
          if (!cancelled) setPoolBalances(balances);
        });
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) {
          setDataLoading(false);
          setLoadProgress(null);
        }
      });

    return () => { cancelled = true; };
  }, [activeAccountId]);

  // Fetch validators in background
  useEffect(() => {
    let cancelled = false;
    setValidatorsLoading(true);
    getValidators()
      .then((v) => { if (!cancelled) setValidators(v); })
      .catch(() => {})
      .finally(() => { if (!cancelled) setValidatorsLoading(false); });
    return () => { cancelled = true; };
  }, []);

  const highlightedPools = useMemo(
    () => poolBalances.filter((p) => p.staked !== "0" || p.unstaked !== "0"),
    [poolBalances],
  );
  const sortedSigningAccounts = useMemo(
    () => sortAccounts(signingAccounts),
    [signingAccounts, sortAccounts],
  );
  const signerAccountOptions = useMemo(() => {
    const options = sortedSigningAccounts.map((entry) => ({
      account_id: entry.account_id,
      label: signingAccountOptionLabel(entry),
    }));
    if (activeAccountId && !sortedSigningAccounts.some((entry) => entry.account_id === activeAccountId)) {
      options.unshift({
        account_id: activeAccountId,
        label: stakingWatchlistOptionLabel({
          network: networkId,
          account_id: activeAccountId,
          added_at_ms: 0,
          source: "manual",
        }),
      });
    }
    return options;
  }, [activeAccountId, networkId, sortedSigningAccounts]);

  const openLedgerHdPathPanel = useCallback(() => {
    setLedgerTiedToSelectedAccount(false);
    setActiveSignerTab("ledger");
    setShowLedgerPanel(true);
    setShowManageSigner(true);
    setHardwareMsg(null);
    setHardwareError(null);
    setLedgerNotice(null);
    setActionError(null);
    setError(null);
  }, []);

  const highlightedPoolIds = useMemo(
    () => new Set(poolBalances.map((p) => p.poolId)),
    [poolBalances],
  );

  const otherValidators = useMemo(
    () => validators.filter((v) => !highlightedPoolIds.has(v.account_id)),
    [validators, highlightedPoolIds],
  );

  const handleImport = useCallback(
    async (target?: SigningKeyEntry, overwrite = false) => {
      if (!canPersistSecureStore) {
        setError("OS secure storage is unavailable on this platform.");
        return;
      }
      setImporting(true);
      setImportingKeyId(
        target ? `${target.account_id}:${target.public_key}` : undefined,
      );
      setImportMsg(null);
      setError(null);
      const sourceHints =
        target?.available_sources.filter(
          (s) => s !== "nearxd_keychain" && s !== "hardware_wallet",
        ) ??
        importSources;
      const sources = sourceHints.length > 0 ? sourceHints : importSources;
      const params = buildImportParams(signingCapabilities, {
        network: networkId,
        account_id: target?.account_id,
        public_key: target?.public_key,
        sources,
        overwrite,
      });
      devLog("[Staking] import_near_signing_keys request:", JSON.stringify(params));
      try {
        const result = await importNearSigningKeys(params);
        devLog("[Staking] import_near_signing_keys result:", JSON.stringify(result, null, 2));
        const count = result.imported_count;
        const targetResult = target
          ? [...(result.imported ?? []), ...(result.skipped ?? [])].find(
              (row) =>
                row.account_id === target.account_id && row.public_key === target.public_key,
            )
          : null;
        const keychainReadyForSelectedKey =
          !target ||
          signingCapabilities?.platform !== "macos" ||
          targetResult?.keychain_protection === "biometry_current_set";
        if (result.failed?.length > 0) {
          devWarn("[Staking] import failures:", JSON.stringify(result.failed, null, 2));
        }
        if (target && !keychainReadyForSelectedKey) {
          const fallbackSource =
            target.available_sources.find(
              (source) =>
                source !== "nearxd_keychain" && source !== "hardware_wallet",
            ) ?? null;
          setImportMsg(
            keychainUpgradeFallbackMessage(fallbackSource, signingCapabilities),
          );
        } else {
          setImportMsg(
            count > 0
              ? `Imported ${count} key${count !== 1 ? "s" : ""} to ${storageLabel}`
              : `No new keys imported (${result.skipped?.length ?? 0} skipped, ${result.failed?.length ?? 0} failed)`,
          );
        }
        setLedgerNotice(null);
        setActiveSignerTab("account_key");
        await loadAccounts(activeAccountId || undefined);
        if (target && keychainReadyForSelectedKey) {
          setCredentialSource("nearxd_keychain", true, {
            accountId: target.account_id,
            publicKey: target.public_key,
          });
        } else if (target) {
          const fallbackSource =
            target.available_sources.find(
              (source) =>
                source !== "nearxd_keychain" && source !== "hardware_wallet",
            ) ?? null;
          setCredentialSource(fallbackSource, true, {
            accountId: target.account_id,
            publicKey: target.public_key,
          });
        }
      } catch (err) {
        devError("[Staking] import_near_signing_keys error:", err);
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setImporting(false);
        setImportingKeyId(undefined);
      }
    },
    [
      activeAccountId,
      canPersistSecureStore,
      importSources,
      loadAccounts,
      setCredentialSource,
      signingCapabilities,
      storageLabel,
    ],
  );

  const handleUpgradeToKeychain = useCallback(async () => {
    if (!selectedEntry) return;
    if (sourceUpgradeKind === "repair") {
      setImporting(true);
      setImportingKeyId(signingKeyId(selectedEntry));
      setImportMsg(null);
      setError(null);
      try {
        const result = await reprotectNearSigningKey({
          network: networkId,
          account_id: selectedEntry.account_id,
          public_key: selectedEntry.public_key,
          reason:
            "NEARx needs your approval to enable fingerprint-protected Keychain signing for this signer.",
        });
        devLog("[Staking] reprotect_near_signing_key result:", JSON.stringify(result, null, 2));
        setImportMsg("Enabled fingerprint-protected Keychain for the selected key.");
        await loadAccounts(selectedEntry.account_id);
        setCredentialSource("nearxd_keychain", true, {
          accountId: selectedEntry.account_id,
          publicKey: selectedEntry.public_key,
        });
      } catch (err) {
        devError("[Staking] reprotect_near_signing_key error:", err);
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setImporting(false);
        setImportingKeyId(undefined);
      }
      return;
    }
    await handleImport(selectedEntry, Boolean(selectedEntry.nearxd_keychain_import_required));
  }, [handleImport, loadAccounts, selectedEntry, setCredentialSource, sourceUpgradeKind]);

  const handleAddWatchlistAccount = useCallback(async () => {
    if (!isTauriRuntime()) return;
    const accountId = watchlistInput.trim();
    if (!accountId) return;
    setError(null);
    setHardwareError(null);
    try {
      const result = await addStakingWatchlistAccount({
        network: networkId,
        account_id: accountId,
        source: "manual",
      });
      setWatchlist(result.entries);
      setWatchlistInput("");
      resetSignerUi();
      await selectSignerAccount(accountId);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [resetSignerUi, selectSignerAccount, watchlistInput]);

  const handleRemoveWatchlistAccount = useCallback(async (accountId: string) => {
    if (!isTauriRuntime()) return;
    setError(null);
    try {
      const result = await removeStakingWatchlistAccount({
        network: networkId,
        account_id: accountId,
      });
      setWatchlist(result.entries);
      if (activeAccountId === accountId) {
        const next = result.entries[0]?.account_id ?? "";
        if (next) {
          resetSignerUi();
          await selectSignerAccount(next);
        } else {
          resetSignerUi();
          setPoolBalances([]);
          setSignerSelection({
            accountId: "",
            publicKey: "",
            credentialSource: null,
            remember: false,
          });
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [activeAccountId, resetSignerUi, selectSignerAccount, setSignerSelection]);

  const handleConnectLedger = useCallback(async () => {
    if (!isTauriRuntime()) return;
    const accountId = activeAccountId.trim();
    if (ledgerTiedToSelectedAccount && !accountId) {
      setShowLedgerPanel(true);
      setHardwareError("Select an account above before binding a Ledger key.");
      return;
    }
    if (!canConnectLedger) {
      setShowLedgerPanel(true);
      setHardwareError("Ledger transport is unavailable on this platform.");
      return;
    }
    setLedgerConnecting(true);
    setShowLedgerPanel(true);
    setLedgerConnection(null);
    setConnectedLedgerSnapshot(null);
    setHardwareMsg(null);
    setHardwareError(null);
    setLedgerNotice(null);
    setError(null);
    try {
      const connected = await connectHardwareWallet({
        network: networkId,
        wallet_type: "ledger",
        account_id: ledgerTiedToSelectedAccount ? accountId : undefined,
        derivation_path: ledgerDerivationPath.trim() || DEFAULT_LEDGER_DERIVATION_PATH,
      });
      const snapshot = snapshotFromConnectResult(connected);
      setLedgerConnection(connected);
      setConnectedLedgerSnapshot(snapshot);
      const successMessage =
        connected.account_binding === "implicit_account"
          ? `Ledger path ${connected.derivation_path} added for implicit account ${connected.account_id}.`
          : `Ledger key connected for ${connected.account_id}.`;
      setHardwareMsg(successMessage);
      setLedgerNotice(successMessage);
      if (connected.account_binding === "implicit_account") {
        await ensureWatchlistAccount(connected.account_id, {
          source: "hardware_wallet",
          wallet_type: connected.wallet_type,
          public_key: connected.public_key,
          derivation_path: connected.derivation_path,
        });
      } else {
        await ensureWatchlistAccount(connected.account_id, { source: "manual" });
      }
      resetSignerUi();
      setSignerSelection({
        accountId: connected.account_id,
        publicKey: connected.public_key,
        credentialSource: "hardware_wallet",
      });
      setShowManageSigner(false);
      await loadAccounts(connected.account_id);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      const friendly = ledgerHardwareErrorMessage(msg);
      setShowLedgerPanel(true);
      setHardwareError(friendly);
      setActionError(friendly);
    } finally {
      setLedgerConnecting(false);
    }
  }, [
    canConnectLedger,
    ensureWatchlistAccount,
    ledgerTiedToSelectedAccount,
    ledgerDerivationPath,
    loadAccounts,
    activeAccountId,
    resetSignerUi,
    setSignerSelection,
  ]);

  const switchAwayFromHardware = useCallback(() => {
    if (!selectedEntry) return;
    const fallback = fallbackLocalSource(selectedEntry);
    if (fallback) {
      setCredentialSource(fallback);
      setHardwareError(null);
      setActionError(null);
      setLedgerNotice(null);
    } else {
      setHardwareError("No other local source is available for this key.");
    }
  }, [selectedEntry]);

  function toggleExpand(poolId: string, action: ActionKind) {
    if (!stakingActionReady) {
      setActionError(
        selectedKeychainImportRequired
          ? "Put this key in fingerprint-protected Keychain before staking with Keychain."
          : "Select a full-access signer key with a usable local source before submitting staking actions.",
      );
      return;
    }
    if (expanded?.poolId === poolId && expanded.action === action) {
      setExpanded(null);
    } else {
      setExpanded({ poolId, action });
      setActionAmount("");
      setActionResult(null);
      setActionError(null);
    }
  }

  async function handleAction() {
    if (!expanded || !selectedKey || !activeAccountId) return;
    if (!stakingActionReady) {
      setActionError(
        selectedKeychainImportRequired
          ? "Put this key in fingerprint-protected Keychain before staking with Keychain."
          : "Signer key must be full access and have a usable local source.",
      );
      return;
    }
    setActionLoading(true);
    setActionResult(null);
    setActionError(null);
    setHardwareError(null);

    const needsAmount = expanded.action === "deposit_and_stake" || expanded.action === "unstake" || expanded.action === "withdraw";
    const yoctoAmount = needsAmount ? nearToYocto(actionAmount) : undefined;

    if (needsAmount && (!actionAmount || yoctoAmount === "0")) {
      setActionError("Please enter an amount");
      setActionLoading(false);
      return;
    }

    const labels: Record<ActionKind, string> = {
      deposit_and_stake: `Stake ${actionAmount} NEAR with ${expanded.poolId}`,
      unstake: `Unstake ${actionAmount} NEAR from ${expanded.poolId}`,
      unstake_all: `Unstake all from ${expanded.poolId}`,
      withdraw: `Withdraw ${actionAmount} NEAR from ${expanded.poolId}`,
      withdraw_all: `Withdraw all from ${expanded.poolId}`,
    };

    try {
      const { txHash, broadcastResult } = await executeStakingAction({
        action: expanded.action,
        poolId: expanded.poolId,
        signerId: activeAccountId,
        publicKey: selectedKey,
        credentialSource: effectiveCredentialSource ?? undefined,
        amount: yoctoAmount,
        reason: labels[expanded.action],
      });
      const summary = summarizeBroadcastResult(broadcastResult, txHash);
      setActionResult(summary);
      if (summary.success !== false) {
        setDataLoading(true);
        getAccountFull(activeAccountId)
          .then((resp) => {
            if (!resp) return;
            const poolIds = resp.pools.map((p) => p.pool_id);
            return getPoolBalancesBatch(poolIds, activeAccountId);
          })
          .then((balances) => { if (balances) setPoolBalances(balances); })
          .finally(() => setDataLoading(false));
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (err instanceof StakingBroadcastError) {
        setActionResult({
          txHash: err.txHash,
          success: false,
          statusLabel: "BROADCAST ERROR",
        });
      }
      const friendly =
        effectiveCredentialSource === "hardware_wallet"
          ? ledgerHardwareErrorMessage(msg)
          : msg;
      setActionError(friendly);
      if (
        effectiveCredentialSource === "hardware_wallet" &&
        (msg.startsWith("ERR_HARDWARE_") || msg.startsWith("ERR_UNAVAILABLE"))
      ) {
        setShowLedgerPanel(true);
        setHardwareError(friendly);
      }
    } finally {
      setActionLoading(false);
    }
  }

  const cardClass = "rounded-lg border border-gray-200 bg-surface text-sm";
  const headerClass = "border-b border-gray-100 px-4 py-2";
  const inputClass = "flex-1 min-w-0 rounded border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:outline-none";
  const compactSelectClass = "min-w-[18rem] max-w-full rounded border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-900 focus:border-blue-500 focus:outline-none disabled:opacity-50";
  const btnStake = "rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700 disabled:opacity-50";
  const btnUnstake = "rounded bg-yellow-500 px-3 py-1 text-xs font-medium text-white hover:bg-yellow-600 disabled:opacity-50";
  const btnWithdraw = "rounded bg-green-600 px-3 py-1 text-xs font-medium text-white hover:bg-green-700 disabled:opacity-50";
  const signerSummaryStatus = resolveSignerSummaryStatus({
    hardwareError,
    error,
    neutralLabel: "Choose account",
    neutralMessage: activeAccountId
      ? null
      : "Select an account, or add one manually or from Ledger.",
    selectionRequiredMessage:
      activeAccountId && !(selectedEntry || selectedLedgerSnapshot)
        ? `Choose a signer key for ${activeAccountId}.`
        : null,
    sourceNeededMessage:
      activeAccountId && (selectedEntry || selectedLedgerSnapshot) && !effectiveCredentialSource
        ? "Choose a signer with a local source, or import/connect one in Manage signer."
        : null,
    incompatibleLabel: selectedKeychainImportRequired ? "Fingerprint required" : "Limited key",
    incompatibleMessage:
      selectedKeychainImportRequired
        ? "Keychain staking is blocked until this key is in fingerprint-protected Keychain."
        : activeAccountId &&
            effectiveCredentialSource &&
            selectedPermissionKind !== null &&
            selectedPermissionKind !== "full_access"
          ? "Choose a full-access key for staking actions."
          : null,
    advisoryLabel: selectedWeakSourceLabel,
    advisoryMessage: selectedWeakSourceWarning,
    readyLabel: "Ready to stake",
    readyMessage:
      ledgerNotice ??
      importMsg ??
      "Selected account and signer are ready for staking actions.",
  });
  const signerSummaryItems = [
    {
      label: "Account",
      value: activeAccountId ? (
        <span className="font-mono">{activeAccountId}</span>
      ) : (
        <span className="text-gray-400">No account selected</span>
      ),
    },
    {
      label: "Key",
      value: selectedEntry ? (
        <div className="min-w-0">
          {selectedEntry.label?.trim() ? (
            <div
              className="truncate text-sm font-medium text-gray-900"
              title={selectedEntry.label ?? undefined}
            >
              {selectedEntry.label}
            </div>
          ) : null}
          <div
            className="truncate font-mono text-sm text-gray-600"
            title={selectedEntry.public_key}
          >
            {shortPublicKey(selectedEntry.public_key)}
          </div>
        </div>
      ) : selectedLedgerSnapshot ? (
        <div className="min-w-0">
          {selectedLedgerSnapshot.label?.trim() ? (
            <div
              className="truncate text-sm font-medium text-gray-900"
              title={selectedLedgerSnapshot.label ?? undefined}
            >
              {selectedLedgerSnapshot.label}
            </div>
          ) : null}
          <div
            className="truncate font-mono text-sm text-gray-600"
            title={selectedLedgerSnapshot.publicKey}
          >
            {shortPublicKey(selectedLedgerSnapshot.publicKey)}
          </div>
        </div>
      ) : selectedKey ? (
        <span className="font-mono">{shortPublicKey(selectedKey)}</span>
      ) : (
        <span className="text-gray-400">Choose a signer key</span>
      ),
    },
    {
      label: "Permission",
      value: selectedEntry ? (
        permissionLabel(selectedEntry)
      ) : selectedLedgerSnapshot ? (
        permissionLabel({ permission: selectedLedgerSnapshot.permission })
      ) : (
        <span className="text-gray-400">Not selected</span>
      ),
    },
    {
      label: "Source",
      value: selectedSourceLabel,
    },
    {
      label: "Ledger path",
      value: selectedLedgerPath ? (
        <span className="font-mono">{selectedLedgerPath}</span>
      ) : (
        <span className="text-gray-400">Not using Ledger</span>
      ),
    },
  ];
  const accountKeyTab = (
    <div className="space-y-4 px-4 py-4">
        <div>
          <div className="text-xs font-medium uppercase tracking-wide text-gray-500">
            Account & key
          </div>
          <div className="mt-1 text-sm text-gray-500">
            Choose the account and key that drive both staking data and transaction signing.
          </div>
        </div>

      {signerAccountOptions.length > 0 ? (
        <label className="block text-sm text-gray-700">
          <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
            Account
          </span>
          <select
            value={activeAccountId}
            onChange={(e) => void handleActiveAccountChange(e.target.value)}
            className={`${compactSelectClass} w-full min-w-0`}
          >
            {signerAccountOptions.map((entry) => (
              <option key={entry.account_id} value={entry.account_id}>
                {entry.label}
              </option>
            ))}
          </select>
        </label>
      ) : null}

	      {allSignableKeyOptions.length > 0 ? (
	        <label className="block text-sm text-gray-700">
	          <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
	            Key
	          </span>
	          <select
	              value={selectedEntry ? signingKeyId(selectedEntry) : ""}
	              onChange={(e) => {
	                const next = allSignableKeys.find(
	                  (key) => signingKeyId(key) === e.target.value,
	                );
	                if (next) {
	                  handleSelectKey(next);
	                }
	              }}
	              className={`${compactSelectClass} w-full min-w-0`}
	            >
	            {allSignableKeyOptions.map((option) => (
	              <option key={option.id} value={option.id}>
	                {option.label}
	              </option>
	            ))}
	          </select>
	        </label>
	      ) : null}

      <label className="inline-flex items-center gap-2 text-sm text-gray-700">
        <input
          type="checkbox"
          checked={showAllKeys}
          onChange={(e) => setShowAllKeys(e.target.checked)}
          className="rounded border-gray-300"
        />
        Show all keys
      </label>

      {selectedEntry && selectedEntry.available_sources.length > 1 ? (
        <div className="border-t border-gray-100 pt-4">
          <div className="mb-2 text-xs font-medium uppercase tracking-wide text-gray-500">
            Signing source
          </div>
          <div className="flex flex-wrap gap-1.5">
            {selectedEntry.available_sources.map((source) => {
              const active = effectiveCredentialSource === source;
              return (
                <button
                  type="button"
                  key={source}
                  onClick={() => setCredentialSource(source)}
                  className={`rounded px-2 py-1 text-xs ${
                    active
                      ? "bg-blue-600 text-white"
                      : "bg-blue-50 text-blue-700 hover:bg-blue-100"
                  }`}
                >
                  {signerSourceLabel(source, selectedEntry, signingCapabilities)}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}

      {selectedEntry ? (
        <div className="border-t border-gray-100 pt-4">
          <SigningKeyLabelEditor
            entry={selectedEntry}
            network={networkId}
            disabled={accountsLoading || importing || ledgerConnecting}
            onSaved={() => loadAccounts(activeAccountId || undefined)}
          />
        </div>
      ) : null}

      {selectedEntry?.permission.kind === "function_call" && showAllKeys ? (
        <div className="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700">
          Selected key is function-call scoped. Some staking actions may be rejected by on-chain
          access-key permission rules.
        </div>
      ) : null}

      <div className="rounded-lg border border-gray-200 bg-surface/60">
        <div className="border-b border-gray-100 px-4 py-2 text-xs font-medium uppercase tracking-wide text-gray-500">
          Available keys
        </div>
        {accountsLoading ? (
          <div className="px-4 py-3 text-gray-500">Loading accounts&hellip;</div>
        ) : keys.length === 0 ? (
          <div className="px-4 py-6 text-center">
            {activeAccountId ? (
              <p className="mb-3 text-gray-600">
                No signing keys found for <strong>{activeAccountId}</strong>.
              </p>
            ) : (
              <p className="mb-3 text-gray-600">Select an account to load signer keys.</p>
            )}
            <div className="flex justify-center gap-2">
              <button
                type="button"
                onClick={() => setActiveSignerTab("import")}
                className="rounded border border-gray-200 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-50"
              >
                Open import
              </button>
              {canConnectLedger ? (
                <button
                  type="button"
                  onClick={() => {
                    setActiveSignerTab("ledger");
                    setShowLedgerPanel(true);
                  }}
                  className="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
                >
                  Open Ledger
                </button>
              ) : null}
            </div>
          </div>
        ) : visibleKeys.length === 0 ? (
          <div className="px-4 py-6 text-center">
            <p className="mb-2 text-sm text-amber-700">
              Only function-call or unknown-permission keys were found.
            </p>
            <button
              type="button"
              onClick={() => setShowAllKeys(true)}
              className="rounded border border-amber-300 px-3 py-1 text-xs font-medium text-amber-700 hover:bg-amber-50"
            >
              Show all keys
            </button>
          </div>
        ) : (
          <AccountPicker
            keys={visibleKeys}
            selectedAccountId={activeAccountId}
            selectedPublicKey={selectedKey}
            onSelect={handleSelectKey}
            onImport={handleImport}
            importing={importing}
            importingKeyId={importingKeyId}
            signingCapabilities={signingCapabilities}
            isStarred={isStarred}
            onToggleStar={toggleStar}
          />
        )}
      </div>
    </div>
  );

  const importTab = (
    <div className="space-y-4 px-4 py-4">
      <div>
        <div className="text-xs font-medium uppercase tracking-wide text-gray-500">
          Import software keys
        </div>
        <div className="mt-1 text-sm text-gray-500">
          Import software keys for the selected account into {storageLabel.toLowerCase()}.
        </div>
      </div>

      <div className="flex flex-wrap gap-3 text-sm">
        <label
          className={`inline-flex items-center gap-1.5 ${canImportLegacy ? "text-gray-700" : "text-gray-400"}`}
        >
          <input
            type="checkbox"
            checked={importSources.includes("legacy_file")}
            onChange={(e) =>
              setImportSources((prev) =>
                e.target.checked
                  ? prev.includes("legacy_file")
                    ? prev
                    : [...prev, "legacy_file"]
                  : prev.filter((s): s is CredentialSource => s !== "legacy_file"),
              )
            }
            disabled={!canImportLegacy}
            className="rounded border-gray-300"
          />
          {credentialSourceLabel("legacy_file", signingCapabilities)}
        </label>
        <label
          className={`inline-flex items-center gap-1.5 ${canImportNearCli ? "text-gray-700" : "text-gray-400"}`}
        >
          <input
            type="checkbox"
            checked={importSources.includes("near_cli_secure")}
            onChange={(e) =>
              setImportSources((prev) =>
                e.target.checked
                  ? prev.includes("near_cli_secure")
                    ? prev
                    : [...prev, "near_cli_secure"]
                  : prev.filter((s): s is CredentialSource => s !== "near_cli_secure"),
              )
            }
            disabled={!canImportNearCli}
            className="rounded border-gray-300"
          />
          {credentialSourceLabel("near_cli_secure", signingCapabilities)}
        </label>
      </div>

      {secureStoreName ? (
        <div className="text-sm text-gray-500">
          {storageLabel}: {secureStoreName}
        </div>
      ) : null}

      <div className="flex flex-wrap gap-2">
        {hasImportableKeys ? (
          <button
            onClick={() => handleImport()}
            disabled={importing || importSources.length === 0 || !canPersistSecureStore}
            className="inline-flex items-center gap-1 rounded bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            <Download size={14} />
            {importing && !importingKeyId ? "Importing…" : "Import all"}
          </button>
        ) : null}
        {selectedEntry ? (
          <button
            onClick={() =>
              handleImport(selectedEntry, Boolean(selectedEntry.nearxd_keychain_import_required))
            }
            disabled={importing || importSources.length === 0 || !canPersistSecureStore}
            className="inline-flex items-center gap-1 rounded border border-gray-200 px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50"
          >
            {importing && importingKeyId === `${selectedEntry.account_id}:${selectedEntry.public_key}`
              ? "Importing…"
              : "Import selected key"}
          </button>
        ) : null}
      </div>

      {importMsg ? (
        <div className="rounded border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-700">
          <KeyRound size={12} className="mr-1 inline" />
          {importMsg}
        </div>
      ) : null}
    </div>
  );

  const ledgerTab = (
    <div className="space-y-4 px-4 py-4">
      {isTauriRuntime() ? (
        <LedgerConnectionPanel
          open={showLedgerPanel}
          onToggleOpen={() => setShowLedgerPanel((prev) => !prev)}
          canConnect={canConnectLedger}
          connecting={ledgerConnecting}
          onConnect={() => void handleConnectLedger()}
          connectLabel={ledgerTiedToSelectedAccount ? "Connect Ledger" : "Connect & Add HD Path"}
          derivationPath={ledgerDerivationPath}
          onDerivationPathChange={setLedgerDerivationPath}
          tiedToSelectedAccount={ledgerTiedToSelectedAccount}
          onTiedToSelectedAccountChange={(value) => {
            setLedgerTiedToSelectedAccount(value);
            setHardwareMsg(null);
            setHardwareError(null);
            setActionError(null);
          }}
          selectedAccountLabel={activeAccountId || null}
          selectedAccountKind="selected account"
          implicitAccountId={ledgerConnection?.implicit_account_id ?? null}
          publicKey={ledgerConnection?.public_key ?? null}
          accountBinding={ledgerConnection?.account_binding ?? null}
          message={hardwareMsg}
          error={hardwareError}
          errorActions={
            effectiveCredentialSource === "hardware_wallet" ? (
              <div className="inline-flex gap-2">
                <button
                  type="button"
                  onClick={() => void handleConnectLedger()}
                  className="rounded border border-amber-300 px-2 py-0.5 text-[11px] text-amber-700 hover:bg-amber-100"
                >
                  Reconnect Ledger
                </button>
                <button
                  type="button"
                  onClick={switchAwayFromHardware}
                  className="rounded border border-amber-300 px-2 py-0.5 text-[11px] text-amber-700 hover:bg-amber-100"
                >
                  Switch source
                </button>
              </div>
            ) : null
          }
        />
      ) : null}
    </div>
  );

  return (
    <div>
      <h1 className="mb-4 text-xl font-bold">Staking</h1>

      <div className={`${cardClass} mb-4`}>
        <div className="flex items-center justify-between border-b border-gray-100 px-4 py-2">
          <h2 className="text-xs font-medium uppercase text-gray-500">Monitored Accounts</h2>
          {watchlistLoading && <span className="text-xs text-gray-400">Loading…</span>}
        </div>
        <div className="flex items-center gap-2 border-b border-gray-100 px-4 py-2">
          <input
            type="text"
            value={watchlistInput}
            onChange={(e) => setWatchlistInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                void handleAddWatchlistAccount();
              }
            }}
            placeholder="alice.near or account.testnet"
            className={inputClass}
          />
          <button
            type="button"
            onClick={() => void handleAddWatchlistAccount()}
            disabled={!isTauriRuntime() || !watchlistInput.trim()}
            className="shrink-0 rounded bg-blue-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            title="Add monitored account"
          >
            <Plus size={12} className="mr-1 inline" />
            Add
          </button>
          {isTauriRuntime() && (
            <button
              type="button"
              onClick={openLedgerHdPathPanel}
              disabled={!canConnectLedger}
              className="shrink-0 rounded border border-blue-200 px-2.5 py-1.5 text-xs font-medium text-blue-700 hover:bg-blue-50 disabled:opacity-50"
              title="Connect Ledger and add an implicit-account HD path"
            >
              <Usb size={12} className="mr-1 inline" />
              Add HD Path
            </button>
          )}
        </div>
        <div className="max-h-40 overflow-y-auto">
          {watchlist.length === 0 && !activeAccountId ? (
            <div className="px-4 py-3 text-xs text-gray-500">
              Add an account to monitor staking positions.
            </div>
          ) : (
            <>
              {activeAccountId && !watchlist.some((e) => e.account_id === activeAccountId) && (
                <div className="border-b border-gray-100 px-4 py-2">
                  <button
                    type="button"
                    className="font-mono text-xs text-blue-700"
                    onClick={() => void handleActiveAccountChange(activeAccountId)}
                  >
                    {activeAccountId}
                  </button>
                </div>
              )}
              {watchlist.map((entry) => {
                const selected = entry.account_id === activeAccountId;
                return (
                  <div
                    key={`${entry.network}:${entry.account_id}`}
                    className={`flex items-center justify-between border-b border-gray-100 px-4 py-2 last:border-b-0 ${
                      selected ? "bg-blue-600/10" : ""
                    }`}
                  >
                    <button
                      type="button"
                      className={`min-w-0 flex-1 text-left ${selected ? "text-blue-800" : "text-gray-700"}`}
                      onClick={() => {
                        void handleActiveAccountChange(entry.account_id);
                      }}
                    >
                      <div className="font-mono text-xs">{entry.account_id}</div>
                      {stakingWatchlistMeta(entry) && (
                        <div className="mt-0.5 truncate text-[11px] text-gray-400">
                          {stakingWatchlistMeta(entry)}
                        </div>
                      )}
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleRemoveWatchlistAccount(entry.account_id)}
                      disabled={!isTauriRuntime()}
                      className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700 disabled:opacity-40"
                      title="Remove from watchlist"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                );
              })}
            </>
          )}
        </div>
      </div>

      <SignerSummaryCard
        title="Signer"
        statusLabel={signerSummaryStatus.label}
        statusTone={signerSummaryStatus.tone}
        message={signerSummaryStatus.message}
        controls={
          <SignerQuickSelectors
            account={{
              label: "Account",
              value: activeAccountId,
              options: signerAccountOptions.map((entry) => ({
                value: entry.account_id,
                label: entry.label,
              })),
              onChange: (value) => {
                setShowManageSigner(false);
                void handleActiveAccountChange(value);
              },
              disabled: accountsLoading || signerAccountOptions.length === 0,
              placeholder: "No accounts",
            }}
            keyOption={{
              label: "Key",
              value: selectedKeyId,
              options: allSignableKeyOptions.map((option) => ({
                value: option.id,
                label: option.label,
              })),
              onChange: (value) => {
                const next = allSignableKeys.find((key) => signingKeyId(key) === value);
                if (next) {
                  handleSelectKey(next);
                }
              },
              disabled: accountsLoading || allSignableKeyOptions.length === 0,
              placeholder: "No keys",
            }}
            source={{
              label: "Source",
              value: effectiveCredentialSource ?? "",
              options: signerSourceOptions,
              onChange: (value) => setCredentialSource(value as CredentialSource),
              disabled: signerSourceOptions.length <= 1,
              placeholder: "Not selected",
              action: isUpgradeEligible(selectedEntry, effectiveCredentialSource, canPersistSecureStore)
                ? {
                    label: upgradeButtonLabel(
                      signingCapabilities,
                      sourceUpgradeKind ?? "import",
                      Boolean(selectedEntry?.nearxd_keychain_import_required),
                    ),
                    onClick: () => void handleUpgradeToKeychain(),
                    disabled: importing,
                    loading: importing && importingKeyId === selectedKeyId,
                    loadingLabel: upgradeLoadingLabel(
                      sourceUpgradeKind ?? "import",
                      Boolean(selectedEntry?.nearxd_keychain_import_required),
                    ),
                  }
                : undefined,
            }}
          />
        }
        items={signerSummaryItems}
        actions={
          <button
            type="button"
            onClick={() => {
              setActiveSignerTab("account_key");
              setShowManageSigner(true);
            }}
            className="inline-flex items-center gap-2 rounded border border-gray-200 px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50"
          >
            <Settings2 className="size-4" />
            Manage signer
          </button>
        }
      />

      {ledgerNotice ? (
        <div className="mb-4 rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-800">
          <div className="font-medium">
            {selectedLedgerSnapshot?.accountBinding === "implicit_account"
              ? "Implicit account added"
              : "Ledger connected"}
          </div>
          <div className="mt-1">{ledgerNotice}</div>
          {dataLoading && activeAccountId ? (
            <div className="mt-2 text-emerald-700">
              Scanning delegations for <span className="font-mono">{activeAccountId}</span>
              {loadProgress ? ` (${loadProgress.done}/${loadProgress.total} pools)` : "…"}
            </div>
          ) : null}
        </div>
      ) : null}

      <ManageSignerPanel
        open={showManageSigner}
        onClose={() => setShowManageSigner(false)}
        activeTab={activeSignerTab}
        onTabChange={(tab) => {
          setActiveSignerTab(tab);
          if (tab === "ledger") {
            setShowLedgerPanel(true);
            setHardwareMsg(null);
            setHardwareError(null);
          }
        }}
      >
        {activeSignerTab === "account_key" ? accountKeyTab : null}
        {activeSignerTab === "import" ? importTab : null}
        {activeSignerTab === "ledger" ? ledgerTab : null}
      </ManageSignerPanel>

      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-surface px-4 py-3 text-sm text-red-600">
          {error}
        </div>
      )}

      {/* Loading progress */}
      {dataLoading && loadProgress && (
        <div className="mb-4 text-sm text-gray-500">
          Loading balances&hellip; ({loadProgress.done}/{loadProgress.total} pools)
        </div>
      )}
      {dataLoading && !loadProgress && (
        <div className="mb-4 text-sm text-gray-500">Loading&hellip;</div>
      )}

      {/* Highlighted Validators */}
      {highlightedPools.length > 0 && (
        <div className={`${cardClass} mb-4`}>
          <div className={headerClass}>
            <h2 className="text-xs font-medium uppercase text-gray-500">Your Staked Validators</h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-100 text-left text-xs text-gray-500">
                  <th className="px-4 py-2 font-medium">Validator</th>
                  <th className="px-4 py-2 font-medium text-right">Staked</th>
                  <th className="px-4 py-2 font-medium text-right">Unstaked</th>
                  <th className="px-4 py-2 font-medium text-center">Withdraw</th>
                  <th className="px-4 py-2 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {highlightedPools.map((pool) => (
                  <HighlightedRow
                    key={pool.poolId}
                    pool={pool}
                    expanded={expanded}
                    actionAmount={actionAmount}
                    actionLoading={actionLoading}
                    actionResult={actionResult}
                    actionError={actionError}
                    onToggle={toggleExpand}
                    onAmountChange={setActionAmount}
                    onConfirm={handleAction}
                    btnStake={btnStake}
                    btnUnstake={btnUnstake}
                    btnWithdraw={btnWithdraw}
                    inputClass={inputClass}
                    actionsDisabled={!stakingActionReady}
                  />
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* No staking positions */}
      {!dataLoading && activeAccountId && highlightedPools.length === 0 && poolBalances.length >= 0 && (
        <div className={`${cardClass} mb-4`}>
          <div className="px-4 py-4 text-center text-sm text-gray-500">
            No active staking positions. Choose a validator below to stake.
          </div>
        </div>
      )}

      {/* Other Validators */}
      <div className={cardClass}>
        <div className={headerClass}>
          <h2 className="text-xs font-medium uppercase text-gray-500">
            Active Validators
            {validatorsLoading && " (loading\u2026)"}
            {!validatorsLoading && ` (${otherValidators.length})`}
          </h2>
        </div>
        {otherValidators.length > 0 ? (
          <div className="overflow-x-auto max-h-96 overflow-y-auto">
            <table className="w-full text-sm">
              <thead className="sticky top-0 bg-surface">
                <tr className="border-b border-gray-100 text-left text-xs text-gray-500">
                  <th className="px-4 py-2 font-medium">Validator</th>
                  <th className="px-4 py-2 font-medium text-right">Total Stake</th>
                  <th className="px-4 py-2 font-medium text-right">Uptime</th>
                  <th className="px-4 py-2 font-medium text-right">Action</th>
                </tr>
              </thead>
              <tbody>
                {otherValidators.map((v) => {
                  const uptime =
                    v.num_expected_blocks > 0
                      ? ((v.num_produced_blocks / v.num_expected_blocks) * 100).toFixed(1)
                      : "—";
                  const isExpanded = expanded?.poolId === v.account_id && expanded.action === "deposit_and_stake";
                  return (
                    <ValidatorRow
                      key={v.account_id}
                      validator={v}
                      uptime={uptime}
                      isExpanded={isExpanded}
                      actionAmount={actionAmount}
                      actionLoading={actionLoading}
                      actionResult={actionResult}
                      actionError={actionError}
                      onToggle={toggleExpand}
                      onAmountChange={setActionAmount}
                      onConfirm={handleAction}
                      btnStake={btnStake}
                      inputClass={inputClass}
                      actionsDisabled={!stakingActionReady}
                    />
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          !validatorsLoading && (
            <div className="px-4 py-4 text-center text-sm text-gray-500">
              No validators loaded.
            </div>
          )
        )}
      </div>
    </div>
  );
}

function HighlightedRow({
  pool,
  expanded,
  actionAmount,
  actionLoading,
  actionResult,
  actionError,
  onToggle,
  onAmountChange,
  onConfirm,
  btnStake,
  btnUnstake,
  btnWithdraw,
  inputClass,
  actionsDisabled,
}: {
  pool: PoolBalance;
  expanded: ExpandedRow | null;
  actionAmount: string;
  actionLoading: boolean;
  actionResult: BroadcastSummary | null;
  actionError: string | null;
  onToggle: (poolId: string, action: ActionKind) => void;
  onAmountChange: (v: string) => void;
  onConfirm: () => void;
  btnStake: string;
  btnUnstake: string;
  btnWithdraw: string;
  inputClass: string;
  actionsDisabled: boolean;
}) {
  const isExpanded = expanded?.poolId === pool.poolId;
  const activeAction = isExpanded ? expanded!.action : null;

  // Determine max amount based on action type
  const maxYocto = activeAction === "unstake" ? pool.staked
    : activeAction === "withdraw" ? pool.unstaked
    : undefined;

  return (
    <>
      <tr className="border-b border-gray-100">
        <td className="px-4 py-2">
          <AccountId accountId={pool.poolId} maxLength="auto" />
        </td>
        <td className="px-4 py-2 text-right font-mono">
          <NearAmount yoctoNear={pool.staked} />
        </td>
        <td className="px-4 py-2 text-right font-mono">
          <NearAmount yoctoNear={pool.unstaked} />
        </td>
        <td className="px-4 py-2 text-center">
          {pool.canWithdraw === true ? (
            <span className="text-green-600 text-xs">Yes</span>
          ) : pool.canWithdraw === false ? (
            <span className="text-gray-400 text-xs">No</span>
          ) : (
            <span className="text-gray-300 text-xs">—</span>
          )}
        </td>
        <td className="px-4 py-2 text-right">
          <div className="flex items-center justify-end gap-1">
            <button
              onClick={() => onToggle(pool.poolId, "deposit_and_stake")}
              className={btnStake}
              disabled={actionsDisabled}
            >
              Stake
            </button>
            {pool.staked !== "0" && (
              <button
                onClick={() => onToggle(pool.poolId, "unstake")}
                className={btnUnstake}
                disabled={actionsDisabled}
              >
                Unstake
              </button>
            )}
            {pool.unstaked !== "0" && pool.canWithdraw && (
              <button
                onClick={() => onToggle(pool.poolId, "withdraw")}
                className={btnWithdraw}
                disabled={actionsDisabled}
              >
                Withdraw
              </button>
            )}
          </div>
        </td>
      </tr>
      {isExpanded && (
        <tr className="border-b border-gray-100 bg-gray-50">
          <td colSpan={5} className="px-4 py-3">
            <ActionPanel
              action={activeAction!}
              poolId={pool.poolId}
              amount={actionAmount}
              maxYocto={maxYocto}
              loading={actionLoading}
              result={actionResult}
              error={actionError}
              onAmountChange={onAmountChange}
              onConfirm={onConfirm}
              onCancel={() => onToggle(pool.poolId, activeAction!)}
              btnConfirm={activeAction === "unstake" || activeAction === "unstake_all" ? btnUnstake : activeAction === "withdraw" || activeAction === "withdraw_all" ? btnWithdraw : btnStake}
              inputClass={inputClass}
            />
          </td>
        </tr>
      )}
    </>
  );
}

function ValidatorRow({
  validator,
  uptime,
  isExpanded,
  actionAmount,
  actionLoading,
  actionResult,
  actionError,
  onToggle,
  onAmountChange,
  onConfirm,
  btnStake,
  inputClass,
  actionsDisabled,
}: {
  validator: ValidatorInfo;
  uptime: string;
  isExpanded: boolean;
  actionAmount: string;
  actionLoading: boolean;
  actionResult: BroadcastSummary | null;
  actionError: string | null;
  onToggle: (poolId: string, action: ActionKind) => void;
  onAmountChange: (v: string) => void;
  onConfirm: () => void;
  btnStake: string;
  inputClass: string;
  actionsDisabled: boolean;
}) {
  return (
    <>
      <tr className="border-b border-gray-100">
        <td className="px-4 py-2">
          <AccountId accountId={validator.account_id} maxLength="auto" />
        </td>
        <td className="px-4 py-2 text-right font-mono">
          <NearAmount yoctoNear={validator.stake} />
        </td>
        <td className="px-4 py-2 text-right">{uptime}%</td>
        <td className="px-4 py-2 text-right">
          <button
            onClick={() => onToggle(validator.account_id, "deposit_and_stake")}
            className={btnStake}
            disabled={actionsDisabled}
          >
            {isExpanded ? <ChevronUp className="size-3 inline" /> : "Stake"}
          </button>
        </td>
      </tr>
      {isExpanded && (
        <tr className="border-b border-gray-100 bg-gray-50">
          <td colSpan={4} className="px-4 py-3">
            <ActionPanel
              action="deposit_and_stake"
              poolId={validator.account_id}
              amount={actionAmount}
              loading={actionLoading}
              result={actionResult}
              error={actionError}
              onAmountChange={onAmountChange}
              onConfirm={onConfirm}
              onCancel={() => onToggle(validator.account_id, "deposit_and_stake")}
              btnConfirm={btnStake}
              inputClass={inputClass}
            />
          </td>
        </tr>
      )}
    </>
  );
}

function ActionPanel({
  action,
  poolId,
  amount,
  maxYocto,
  loading,
  result,
  error,
  onAmountChange,
  onConfirm,
  onCancel,
  btnConfirm,
  inputClass,
}: {
  action: ActionKind;
  poolId: string;
  amount: string;
  maxYocto?: string;
  loading: boolean;
  result: BroadcastSummary | null;
  error: string | null;
  onAmountChange: (v: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
  btnConfirm: string;
  inputClass: string;
}) {
  const needsAmount = action === "deposit_and_stake" || action === "unstake" || action === "withdraw";
  const labels: Record<ActionKind, string> = {
    deposit_and_stake: "Stake",
    unstake: "Unstake",
    unstake_all: "Unstake All",
    withdraw: "Withdraw",
    withdraw_all: "Withdraw All",
  };

  const maxNear = maxYocto && maxYocto !== "0" ? yoctoToNear(maxYocto) : undefined;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-xs text-gray-500">
        <span className="font-medium">{labels[action]}</span>
        <span>&rarr;</span>
        <span className="font-mono">{poolId}</span>
      </div>
      {needsAmount && (
        <div className="flex items-center gap-2">
          <input
            className={`${inputClass} max-w-48`}
            value={amount}
            onChange={(e) => onAmountChange(e.target.value)}
            placeholder="0.0"
            type="text"
            disabled={loading}
          />
          <span className="text-xs text-gray-500">NEAR</span>
          {maxNear && (
            <button
              type="button"
              onClick={() => onAmountChange(maxNear)}
              disabled={loading}
              className="rounded border border-gray-300 px-2 py-1 text-xs font-medium text-gray-600 hover:bg-gray-100 disabled:opacity-50"
            >
              Max
            </button>
          )}
        </div>
      )}
      {maxNear && needsAmount && (
        <div className="text-xs text-gray-400">
          Available: {maxNear} NEAR
        </div>
      )}
      <div className="flex items-center gap-2">
        <button onClick={onConfirm} disabled={loading} className={btnConfirm}>
          {loading ? "Processing\u2026" : `Confirm ${labels[action]}`}
        </button>
        <button onClick={onCancel} disabled={loading} className="text-xs text-gray-500 hover:text-gray-700">
          Cancel
        </button>
      </div>
      {result && (
        <div
          className={`text-xs ${result.success === false ? "text-rose-600" : "text-green-600"}`}
        >
          {result.success === false ? "Broadcast failed" : "Success!"} TX:{" "}
          <Link
            to={`/tx/${result.txHash}`}
            className="break-all text-blue-600 hover:underline font-mono"
            title={result.txHash}
          >
            {result.txHash}
          </Link>
          <span className="ml-2 text-[11px] uppercase tracking-wide text-gray-500">
            {result.statusLabel}
          </span>
        </div>
      )}
      {error && <div className="text-xs text-red-600">{error}</div>}
    </div>
  );
}
