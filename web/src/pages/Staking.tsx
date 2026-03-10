import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Check, ChevronUp, Plus, Trash2, Usb } from "lucide-react";
import {
  listStakingWatchlist,
  addStakingWatchlistAccount,
  removeStakingWatchlistAccount,
  connectHardwareWallet,
  getSigningCapabilitiesCached,
  isTauriRuntime,
} from "../tauri/runtime";
import type {
  ConnectHardwareWalletResult,
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
import SignerSummaryCard from "../components/SignerSummaryCard";
import SignerQuickSelectors from "../components/SignerQuickSelectors";
import CopyableValue from "../components/CopyableValue";
import { useAccountPrefs } from "../hooks/useAccountPrefs";
import useSignerSelection from "../hooks/useSignerSelection";
import { resolveSignerSummaryStatus } from "../lib/signerSummaryStatus";
import type { BroadcastSummary } from "../lib/broadcastSummary";
import { summarizeBroadcastResult } from "../lib/broadcastSummary";
import {
  permissionLabel,
  signingAccountOptionLabel,
  signingKeyOptionLabel,
  stakingWatchlistMeta,
  stakingWatchlistOptionLabel,
} from "../lib/hardwareWalletDisplay";
import {
  preferSigningKey,
  signingKeyId,
} from "../lib/signerSourceSelection";
import {
  ledgerHardwareErrorMessage,
  snapshotFromConnectResult,
  type ConnectedLedgerSnapshot,
} from "../lib/ledgerConnectionUi";
import LedgerConnectionPanel from "../components/LedgerConnectionPanel";

type ActionKind = "deposit_and_stake" | "unstake" | "unstake_all" | "withdraw" | "withdraw_all";

interface ExpandedRow {
  poolId: string;
  action: ActionKind;
}

const NEAR_DECIMALS = 24;
const DEV_LOG = import.meta.env.DEV;

function devWarn(...args: unknown[]) {
  if (DEV_LOG) console.warn(...args);
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
    starredAccounts,
    lastAccountId,
    lastPublicKey,
    setLastAccount,
    setLastKey,
    sortAccounts,
    toggleStar,
  } = useAccountPrefs("staking");
  const [showAllKeys] = useState(false);
  const [signingCapabilities, setSigningCapabilities] = useState<SigningCapabilities | null>(null);
  const [showLedgerPanel, setShowLedgerPanel] = useState(false);
  const [ledgerTiedToSelectedAccount, setLedgerTiedToSelectedAccount] = useState(false);
  const [ledgerConnection, setLedgerConnection] =
    useState<ConnectHardwareWalletResult | null>(null);
  const [hardwareMsg, setHardwareMsg] = useState<string | null>(null);
  const [hardwareError, setHardwareError] = useState<string | null>(null);
  const [ledgerConnecting, setLedgerConnecting] = useState(false);
  const [ledgerDerivationPath, setLedgerDerivationPath] = useState("44'/397'/0'/0'/1'");

  const [poolBalances, setPoolBalances] = useState<PoolBalance[]>([]);
  const [validators, setValidators] = useState<ValidatorInfo[]>([]);
  const [loadProgress, setLoadProgress] = useState<{ done: number; total: number } | null>(null);
  const [dataLoading, setDataLoading] = useState(false);
  const [validatorsLoading, setValidatorsLoading] = useState(false);
  const [validatorsError, setValidatorsError] = useState<string | null>(null);

  const [expanded, setExpanded] = useState<ExpandedRow | null>(null);
  const [actionAmount, setActionAmount] = useState("");
  const [actionLoading, setActionLoading] = useState(false);
  const [actionResult, setActionResult] = useState<BroadcastSummary | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const [error, setError] = useState<string | null>(null);
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
    selectedEntry,
    selectedKeyId,
    selectedPublicKey: selectedKey,
    setSelection: setSignerSelection,
    signingAccounts,
  } = useSignerSelection({
    network: networkId,
    initialAccountId: initialAccount ?? lastAccountId ?? "",
    lastAccountId,
    lastPublicKey,
    sortAccounts,
    setLastAccount,
    setLastKey,
    autoLoad: false,
  });

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

  const selectedLedgerSnapshot = useMemo(() => {
    if (!connectedLedgerSnapshot) {
      return null;
    }
    const matchesSelectedKey =
      activeAccountId.trim() === connectedLedgerSnapshot.accountId &&
      selectedKey.trim() === connectedLedgerSnapshot.publicKey;
    const matchesAccount =
      activeAccountId.trim() === connectedLedgerSnapshot.accountId;
    return matchesSelectedKey || matchesAccount ? connectedLedgerSnapshot : null;
  }, [
    activeAccountId,
    connectedLedgerSnapshot,
    selectedKey,
  ]);
  const hasUsableSource = Boolean(
    (selectedEntry && selectedEntry.available_sources.length > 0) || selectedLedgerSnapshot,
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
  const stakingActionReady = Boolean(
    (selectedEntry || selectedLedgerSnapshot) &&
    activeAccountId &&
    hasUsableSource &&
    (selectedPermissionKind === "full_access" || selectedPermissionKind === "unknown"),
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
  const [validatorsRetry, setValidatorsRetry] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setValidatorsLoading(true);
    setValidatorsError(null);
    getValidators()
      .then((v) => { if (!cancelled) setValidators(v); })
      .catch((err) => {
        console.error("[staking] failed to load validators:", err);
        if (!cancelled) setValidatorsError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => { if (!cancelled) setValidatorsLoading(false); });
    return () => { cancelled = true; };
  }, [validatorsRetry]);

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
    setShowLedgerPanel(true);
    setHardwareMsg(null);
    setHardwareError(null);
    setLedgerNotice(null);
    setActionError(null);
    setError(null);
  }, []);

  const handleConnectLedger = useCallback(async () => {
    setLedgerConnecting(true);
    setHardwareMsg(null);
    setHardwareError(null);
    try {
      const result = await connectHardwareWallet({
        network: networkId,
        wallet_type: "ledger",
        account_id: ledgerTiedToSelectedAccount ? activeAccountId || undefined : undefined,
        derivation_path: ledgerDerivationPath,
      });
      setLedgerConnection(result);
      const snapshot = snapshotFromConnectResult(result);
      setConnectedLedgerSnapshot(snapshot);

      const derivedAccountId = result.account_id;
      if (isTauriRuntime()) {
        try {
          const updated = await addStakingWatchlistAccount({
            network: networkId,
            account_id: derivedAccountId,
            source: "hardware_wallet",
            wallet_type: "ledger",
            public_key: result.public_key,
            derivation_path: result.derivation_path,
          });
          setWatchlist(updated.entries);
        } catch {
          // watchlist add is best-effort
        }
      }

      resetSignerUi();
      await loadAccounts(derivedAccountId);
      setHardwareMsg(
        `Connected Ledger: ${derivedAccountId} (${result.derivation_path})`,
      );
    } catch (err) {
      const raw = err instanceof Error ? err.message : String(err);
      setHardwareError(ledgerHardwareErrorMessage(raw));
    } finally {
      setLedgerConnecting(false);
    }
  }, [
    activeAccountId,
    ledgerDerivationPath,
    ledgerTiedToSelectedAccount,
    loadAccounts,
    resetSignerUi,
  ]);

  const highlightedPoolIds = useMemo(
    () => new Set(poolBalances.map((p) => p.poolId)),
    [poolBalances],
  );

  const otherValidators = useMemo(
    () => validators.filter((v) => !highlightedPoolIds.has(v.account_id)),
    [validators, highlightedPoolIds],
  );

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
          });
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [activeAccountId, resetSignerUi, selectSignerAccount, setSignerSelection]);

  function toggleExpand(poolId: string, action: ActionKind) {
    if (!stakingActionReady) {
      setActionError("Select a signer key with full access before submitting staking actions.");
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
      setActionError("Select a full-access signer key.");
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

    const isHardwareKey = Boolean(
      selectedEntry?.hardware_wallet || selectedLedgerSnapshot,
    );
    try {
      const { txHash, broadcastResult } = await executeStakingAction({
        action: expanded.action,
        poolId: expanded.poolId,
        signerId: activeAccountId,
        publicKey: selectedKey,
        amount: yoctoAmount,
        credentialSource: isHardwareKey ? "hardware_wallet" : undefined,
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
      const friendly = isHardwareKey
        ? ledgerHardwareErrorMessage(msg)
        : msg;
      setActionError(friendly);
      if (
        isHardwareKey &&
        (msg.startsWith("ERR_HARDWARE_") || msg.startsWith("ERR_UNAVAILABLE"))
      ) {
        setShowLedgerPanel(true);
        setHardwareError(friendly);
      }
    } finally {
      setActionLoading(false);
    }
  }

  const cardClass = "rounded-lg border border-gray-200 bg-surface shadow-sm text-sm";
  const headerClass = "border-b border-gray-100 px-4 py-3";
  const inputClass = "flex-1 min-w-0 rounded-md border border-gray-300 bg-surface px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20 focus:outline-none";
  const btnStake = "rounded-md bg-blue-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-blue-700 disabled:opacity-50";
  const btnUnstake = "rounded-md bg-yellow-500 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-yellow-600 disabled:opacity-50";
  const btnWithdraw = "rounded-md bg-green-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-green-700 disabled:opacity-50";
  const signerSummaryStatus = resolveSignerSummaryStatus({
    hardwareError,
    error: activeAccountId && (selectedEntry || selectedLedgerSnapshot) && !hasUsableSource
      ? "Selected key has no usable source. Import or connect a key in Settings."
      : activeAccountId && selectedPermissionKind !== null && selectedPermissionKind !== "full_access" && selectedPermissionKind !== "unknown"
        ? "Choose a full-access key for staking actions."
        : error,
    neutralLabel: "Choose account",
    neutralMessage: activeAccountId
      ? (!(selectedEntry || selectedLedgerSnapshot)
          ? `Choose a signer key for ${activeAccountId}.`
          : null)
      : "Select an account, or add one manually or from Ledger.",
    readyLabel: "Ready to stake",
    readyMessage: ledgerNotice ?? "Signer is ready.",
  });
  const selectedPublicKey =
    selectedEntry?.public_key ?? selectedLedgerSnapshot?.publicKey ?? selectedKey ?? null;

  const signerSummaryItems = [
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
      label: "Ledger path",
      value: selectedLedgerPath ? (
        <span className="font-mono">{selectedLedgerPath}</span>
      ) : (
        <span className="text-gray-400">Not using Ledger</span>
      ),
    },
  ];

  return (
    <div>
      <h1 className="mb-4 text-2xl font-bold tracking-tight text-gray-900">Staking</h1>

      <div className={`${cardClass} mb-4`}>
        <div className="flex items-center justify-between border-b border-gray-100 px-4 py-3">
          <h2 className="text-xs font-medium uppercase tracking-wide text-gray-500">Monitored Accounts</h2>
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
            className="shrink-0 rounded-md bg-blue-600 px-2.5 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-700 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:outline-none disabled:opacity-50"
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
              className="shrink-0 rounded-md border border-blue-200 px-2.5 py-1.5 text-xs font-medium text-blue-700 transition-colors hover:bg-blue-50 focus:ring-2 focus:ring-blue-500/20 focus:outline-none disabled:opacity-50"
              title="Connect Ledger and add an implicit-account HD path"
            >
              <Usb size={12} className="mr-1 inline" />
              Add HD Path
            </button>
          )}
        </div>
        <div className="max-h-56 overflow-y-auto">
          {watchlist.length === 0 && !activeAccountId ? (
            <div className="px-4 py-3 text-xs text-gray-500">
              Add an account to monitor staking positions.
            </div>
          ) : (
            <>
              {activeAccountId && !watchlist.some((e) => e.account_id === activeAccountId) && (
                <button
                  type="button"
                  className="flex w-full cursor-pointer items-center gap-2 border-b border-gray-100 border-l-2 border-l-blue-600 px-4 py-2 text-left"
                  onClick={() => void handleActiveAccountChange(activeAccountId)}
                >
                  <Check size={12} className="shrink-0 text-blue-600" />
                  <span className="min-w-0 flex-1 font-mono text-xs text-blue-600">{activeAccountId}</span>
                </button>
              )}
              {watchlist.map((entry) => {
                const selected = entry.account_id === activeAccountId;
                return (
                  <div
                    key={`${entry.network}:${entry.account_id}`}
                    className={`flex items-center border-b border-gray-100 transition-colors last:border-b-0 ${
                      selected
                        ? "border-l-2 border-l-blue-600 bg-blue-50/60"
                        : "border-l-2 border-l-transparent hover:bg-gray-50"
                    }`}
                  >
                    <button
                      type="button"
                      className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 px-4 py-2 text-left"
                      onClick={() => {
                        void handleActiveAccountChange(entry.account_id);
                      }}
                    >
                      {selected ? (
                        <Check size={12} className="shrink-0 text-blue-600" />
                      ) : (
                        <span className="size-3 shrink-0" />
                      )}
                      <span className="min-w-0 flex-1">
                        <span className={`block font-mono text-xs ${selected ? "text-blue-600" : "text-gray-700"}`}>
                          {entry.account_id}
                        </span>
                        {stakingWatchlistMeta(entry) && (
                          <span className="mt-0.5 block break-all text-[11px] text-gray-400">
                            {stakingWatchlistMeta(entry)}
                          </span>
                        )}
                      </span>
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleRemoveWatchlistAccount(entry.account_id)}
                      disabled={!isTauriRuntime()}
                      className="mr-2 rounded p-1 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-700 disabled:opacity-40"
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
        {showLedgerPanel && isTauriRuntime() && (
          <LedgerConnectionPanel
            open={showLedgerPanel}
            onToggleOpen={() => setShowLedgerPanel((v) => !v)}
            canConnect={canConnectLedger}
            connecting={ledgerConnecting}
            onConnect={() => void handleConnectLedger()}
            derivationPath={ledgerDerivationPath}
            onDerivationPathChange={setLedgerDerivationPath}
            tiedToSelectedAccount={ledgerTiedToSelectedAccount}
            onTiedToSelectedAccountChange={setLedgerTiedToSelectedAccount}
            selectedAccountLabel={activeAccountId}
            selectedAccountKind="staking account"
            implicitAccountId={ledgerConnection?.implicit_account_id}
            publicKey={ledgerConnection?.public_key}
            accountBinding={ledgerConnection?.account_binding}
            message={hardwareMsg}
            error={hardwareError}
          />
        )}
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
                void handleActiveAccountChange(value);
              },
              disabled: accountsLoading || signerAccountOptions.length === 0,
              placeholder: "No accounts",
              starredValues: new Set(starredAccounts),
              onToggleStar: toggleStar,
              meta: activeAccountId ? (
                <CopyableValue text={activeAccountId}>
                  <Link to={`/account/${activeAccountId}`} className="text-blue-600 hover:underline">
                    View in Explorer
                  </Link>
                </CopyableValue>
              ) : undefined,
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
              meta: selectedPublicKey ? (
                <CopyableValue text={selectedPublicKey}>
                  <span className="break-all font-mono text-gray-500">{selectedPublicKey}</span>
                </CopyableValue>
              ) : undefined,
            }}
          />
        }
        items={signerSummaryItems}
        actions={
          <Link
            to="/settings"
            className="inline-flex items-center gap-2 rounded-md border border-gray-300 px-3 py-1.5 text-sm font-medium text-gray-700 transition-colors hover:bg-gray-50 focus:ring-2 focus:ring-blue-500/20 focus:outline-none"
          >
            Manage keys in Settings &rarr;
          </Link>
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

      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-rose-50 px-4 py-3 text-sm text-rose-700">
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
            <h2 className="text-xs font-medium uppercase tracking-wide text-gray-500">Your Staked Validators</h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-100 text-left text-xs text-gray-500">
                  <th className="px-4 py-2.5 font-medium">Validator</th>
                  <th className="px-4 py-2.5 font-medium text-right">Staked</th>
                  <th className="px-4 py-2.5 font-medium text-right">Unstaked</th>
                  <th className="px-4 py-2.5 font-medium text-center">Withdraw</th>
                  <th className="px-4 py-2.5 font-medium text-right">Actions</th>
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
          <h2 className="text-xs font-medium uppercase tracking-wide text-gray-500">
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
                  <th className="px-4 py-2.5 font-medium">Validator</th>
                  <th className="px-4 py-2.5 font-medium text-right">Total Stake</th>
                  <th className="px-4 py-2.5 font-medium text-right">Uptime</th>
                  <th className="px-4 py-2.5 font-medium text-right">Action</th>
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
              {validatorsError ? (
                <div className="space-y-2">
                  <div className="text-red-500">Failed to load validators: {validatorsError}</div>
                  <button
                    className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700"
                    onClick={() => setValidatorsRetry((n) => n + 1)}
                  >
                    Retry
                  </button>
                </div>
              ) : (
                "No validators loaded."
              )}
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
              btnConfirm={btnStake}
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
              className="rounded-md border border-gray-300 px-2 py-1 text-xs font-medium text-gray-600 transition-colors hover:bg-gray-100 focus:ring-2 focus:ring-blue-500/20 focus:outline-none disabled:opacity-50"
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
          className={`text-xs ${result.success === false ? "text-rose-600" : result.success === true ? "text-green-600" : "text-yellow-600"}`}
        >
          {result.success === false ? "Broadcast failed" : result.success === true ? "Success!" : "Submitted"} TX:{" "}
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
