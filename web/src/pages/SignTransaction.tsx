import { useState, useEffect, useCallback, useMemo } from "react";
import { Download, KeyRound, Settings2 } from "lucide-react";
import { viewAccessKey, broadcastTransaction } from "../api/rpc";
import {
  connectHardwareWallet,
  getSigningCapabilitiesCached,
  importNearSigningKeys,
  reprotectNearSigningKey,
  isTauriRuntime,
  signTransaction,
} from "../tauri/runtime";
import type {
  AccessPermission,
  ConnectHardwareWalletResult,
  CredentialSource,
  SignTransactionResult,
  SigningCapabilities,
  SigningKeyEntry,
} from "../tauri/runtime";
import { networkId } from "../config";
import { useAccountPrefs } from "../hooks/useAccountPrefs";
import useSignerSelection from "../hooks/useSignerSelection";
import AccountPicker from "../components/AccountPicker";
import LedgerConnectionPanel from "../components/LedgerConnectionPanel";
import SigningKeyLabelEditor from "../components/SigningKeyLabelEditor";
import ManageSignerPanel from "../components/ManageSignerPanel";
import SignerSummaryCard from "../components/SignerSummaryCard";
import SignerQuickSelectors from "../components/SignerQuickSelectors";
import DualUnitInput from "../components/DualUnitInput";
import SignTransactionConfirmationModal from "../components/SignTransactionConfirmationModal";
import type { AccountPickerKeyBadge } from "../components/AccountPicker";
import {
  availableImportSources,
  buildImportParams,
  credentialSourceLabel,
  DEFAULT_LEDGER_DERIVATION_PATH,
  secureStorageLabel,
  secureStoreBackendLabel,
} from "../tauri/signingCapabilities";
import {
  signingAccountOptionLabel,
  permissionLabel,
  signingKeyOptionLabel,
  shortPublicKey,
} from "../lib/hardwareWalletDisplay";
import { summarizeBroadcastResult } from "../lib/broadcastSummary";
import { resolveSignerSummaryStatus } from "../lib/signerSummaryStatus";
import {
  keychainUpgradeFallbackMessage,
  isUpgradeEligible,
  resolveSourceUpgradeKind,
  upgradeButtonLabel,
  upgradeLoadingLabel,
} from "../lib/sourceUpgrade";
import {
  fallbackLocalSource,
  keyHasUsableSource,
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

type ActionType = "Transfer" | "FunctionCall";

type KeyCompatibility = {
  compatible: boolean;
  risky: boolean;
  reason: string | null;
};

const rowClass =
  "flex items-baseline gap-3 border-b border-gray-100 px-4 py-3 last:border-b-0";
const labelClass = "shrink-0 w-24 text-right text-gray-500";
const inputClass =
  "flex-1 min-w-0 rounded border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:outline-none";
const toolbarSelectClass =
  "min-w-[14rem] rounded border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-900 focus:border-blue-500 focus:outline-none disabled:opacity-50";
const DEV_LOG = import.meta.env.DEV;
const YOCTO_DECIMALS = 24;
const TGAS_DECIMALS = 12;

function devLog(...args: unknown[]) {
  if (DEV_LOG) console.log(...args);
}

function devError(...args: unknown[]) {
  if (DEV_LOG) console.error(...args);
}

function isNonZeroYocto(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.length > 0 && !/^0+$/.test(trimmed);
}

function decimalToRaw(value: string, decimals: number): string | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return "";
  }
  if (!/^\d*(\.\d*)?$/.test(trimmed)) {
    return null;
  }
  const [whole = "0", fraction = ""] = trimmed.split(".");
  if (fraction.length > decimals) {
    return null;
  }
  const normalizedWhole = whole.replace(/^0+(?=\d)/, "") || "0";
  const raw = `${normalizedWhole}${fraction.padEnd(decimals, "0")}`.replace(/^0+(?=\d)/, "");
  return raw || "0";
}

function rawToDecimal(value: string, decimals: number): string {
  const digits = value.replace(/\D+/g, "").replace(/^0+(?=\d)/, "") || "0";
  const padded = digits.padStart(decimals + 1, "0");
  const whole = padded.slice(0, padded.length - decimals).replace(/^0+(?=\d)/, "") || "0";
  const fraction = padded.slice(padded.length - decimals).replace(/0+$/, "");
  return fraction ? `${whole}.${fraction}` : whole;
}

function normalizeRawInteger(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return "";
  }
  if (!/^\d+$/.test(trimmed)) {
    return null;
  }
  return trimmed.replace(/^0+(?=\d)/, "") || "0";
}

function formatArgsPreview(argsJson: string): { preview: string | null; error: string | null } {
  const trimmed = argsJson.trim();
  if (!trimmed) {
    return {
      preview: null,
      error: "Enter JSON args.",
    };
  }
  try {
    return {
      preview: JSON.stringify(JSON.parse(trimmed), null, 2),
      error: null,
    };
  } catch {
    return {
      preview: trimmed,
      error: "Enter valid JSON args.",
    };
  }
}

function evaluateKeyCompatibility(
  key: { permission: AccessPermission },
  actionType: ActionType,
  receiverId: string,
  methodName: string,
  fnDeposit: string,
): KeyCompatibility {
  if (key.permission.kind === "full_access") {
    return { compatible: true, risky: false, reason: null };
  }

  if (key.permission.kind === "unknown") {
    return {
      compatible: false,
      risky: false,
      reason: "Unknown on-chain access permission (RPC unavailable or unreadable).",
    };
  }

  if (actionType === "Transfer") {
    return {
      compatible: false,
      risky: false,
      reason: "Function-call keys cannot authorize Transfer actions.",
    };
  }

  const receiver = receiverId.trim();
  const method = methodName.trim();
  if (!receiver || receiver !== key.permission.receiver_id) {
    return {
      compatible: false,
      risky: false,
      reason: `Receiver must be ${key.permission.receiver_id || "(missing receiver)"}.`,
    };
  }

  const allowedMethods = key.permission.method_names ?? [];
  if (allowedMethods.length > 0) {
    if (!method) {
      return {
        compatible: false,
        risky: false,
        reason: `Method is required (${allowedMethods.join(", ")}).`,
      };
    }
    if (!allowedMethods.includes(method)) {
      return {
        compatible: false,
        risky: false,
        reason: `Method not permitted (${allowedMethods.join(", ")}).`,
      };
    }
  }

  if (isNonZeroYocto(fnDeposit)) {
    return {
      compatible: false,
      risky: true,
      reason: "Attached deposit is risky for function-call keys.",
    };
  }

  return { compatible: true, risky: false, reason: null };
}

export default function SignTransaction() {
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
  } = useAccountPrefs("sign");
  const [manualMode, setManualMode] = useState(false);
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

  const [signerId, setSignerId] = useState("");
  const [publicKey, setPublicKey] = useState("");
  const [receiverId, setReceiverId] = useState("");
  const [actionType, setActionType] = useState<ActionType>("Transfer");
  const [showManageSigner, setShowManageSigner] = useState(false);
  const [activeSignerTab, setActiveSignerTab] = useState<SignerModalTab>("account_key");
  const [showConfirmModal, setShowConfirmModal] = useState(false);
  const [connectedLedgerSnapshot, setConnectedLedgerSnapshot] =
    useState<ConnectedLedgerSnapshot | null>(null);
  const [ledgerNotice, setLedgerNotice] = useState<string | null>(null);

  const [depositNear, setDepositNear] = useState("0");
  const [deposit, setDeposit] = useState("0");
  const [depositError, setDepositError] = useState<string | null>(null);

  const [methodName, setMethodName] = useState("");
  const [argsJson, setArgsJson] = useState("{}");
  const [gasTgas, setGasTgas] = useState("30");
  const [gas, setGas] = useState("30000000000000");
  const [gasError, setGasError] = useState<string | null>(null);
  const [fnDepositNear, setFnDepositNear] = useState("0");
  const [fnDeposit, setFnDeposit] = useState("0");
  const [fnDepositError, setFnDepositError] = useState<string | null>(null);

  const [signResult, setSignResult] = useState<SignTransactionResult | null>(null);
  const [broadcastResult, setBroadcastResult] = useState<unknown>(null);
  const [broadcastError, setBroadcastError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [pendingAction, setPendingAction] = useState<"sign" | "sign_and_broadcast" | "broadcast" | null>(null);
  const [importing, setImporting] = useState(false);
  const [importingKeyId, setImportingKeyId] = useState<string | undefined>();
  const [importingAccountId, setImportingAccountId] = useState<string | undefined>();
  const [importMsg, setImportMsg] = useState<string | null>(null);

  const {
    accountsLoading,
    allSignableKeys,
    keys,
    load: loadKeys,
    loadError,
    selectAccount: selectSignerAccount,
    selectKey: selectSignerKey,
    selectedAccountId,
    selectedCredentialSource,
    selectedKeyId,
    selectedPublicKey: selectedAccountPublicKey,
    setCredentialSource,
    setSelection: setSignerSelection,
    signingAccounts,
  } = useSignerSelection({
    network: networkId,
    initialAccountId: lastAccountId ?? "",
    lastAccountId,
    lastPublicKey,
    lastCredentialSource,
    getLastSource,
    sortAccounts,
    setLastAccount,
    setLastKey,
    setLastSource,
  });

  const secureStoreName = secureStoreBackendLabel(signingCapabilities);
  const storageLabel = secureStorageLabel();
  const canImportLegacy = signingCapabilities?.supports_legacy_import ?? true;
  const canImportNearCli = signingCapabilities?.supports_near_cli_secure ?? true;
  const canPersistSecureStore =
    signingCapabilities?.supports_secure_store_persistence ?? true;
  const canConnectLedger =
    signingCapabilities?.supports_hardware_wallet_connect ?? true;

  const addDebug = useCallback((msg: string) => {
    devLog("[SignTransaction]", msg);
  }, []);

  const sortedSigningAccounts = useMemo(
    () => sortAccounts(signingAccounts),
    [signingAccounts, sortAccounts],
  );
  const signerAccountOptions = useMemo(() => {
    const options = sortedSigningAccounts.map((entry) => ({
      account_id: entry.account_id,
      label: signingAccountOptionLabel(entry),
    }));
    if (
      selectedAccountId &&
      !sortedSigningAccounts.some((entry) => entry.account_id === selectedAccountId)
    ) {
      options.unshift({
        account_id: selectedAccountId,
        label: selectedAccountId,
      });
    }
    return options;
  }, [selectedAccountId, sortedSigningAccounts]);
  const selectedLedgerAccountId = useMemo(
    () => (selectedAccountId || signerId).trim(),
    [selectedAccountId, signerId],
  );
  const updateTransferNear = useCallback((value: string) => {
    setDepositNear(value);
    const raw = decimalToRaw(value, YOCTO_DECIMALS);
    if (raw === null) {
      setDepositError("Enter a valid NEAR amount.");
      return;
    }
    setDeposit(raw);
    setDepositError(raw && raw !== "0" ? null : "Enter a transfer amount greater than 0.");
  }, []);
  const updateTransferYocto = useCallback((value: string) => {
    setDeposit(value);
    const raw = normalizeRawInteger(value);
    if (raw === null) {
      setDepositError("Enter a valid yocto amount.");
      return;
    }
    setDepositNear(raw ? rawToDecimal(raw, YOCTO_DECIMALS) : "");
    setDepositError(raw && raw !== "0" ? null : "Enter a transfer amount greater than 0.");
  }, []);
  const updateFunctionDepositNear = useCallback((value: string) => {
    setFnDepositNear(value);
    const raw = decimalToRaw(value, YOCTO_DECIMALS);
    if (raw === null) {
      setFnDepositError("Enter a valid NEAR deposit.");
      return;
    }
    setFnDeposit(raw);
    setFnDepositError(null);
  }, []);
  const updateFunctionDepositYocto = useCallback((value: string) => {
    setFnDeposit(value);
    const raw = normalizeRawInteger(value);
    if (raw === null) {
      setFnDepositError("Enter a valid yocto deposit.");
      return;
    }
    setFnDepositNear(raw ? rawToDecimal(raw, YOCTO_DECIMALS) : "");
    setFnDepositError(null);
  }, []);
  const updateGasTgas = useCallback((value: string) => {
    setGasTgas(value);
    const raw = decimalToRaw(value, TGAS_DECIMALS);
    if (raw === null) {
      setGasError("Enter a valid Tgas amount.");
      return;
    }
    if (raw && !Number.isSafeInteger(Number(raw))) {
      setGasError("Gas value is too large.");
      return;
    }
    setGas(raw);
    setGasError(raw && raw !== "0" ? null : "Enter a gas amount greater than 0.");
  }, []);
  const updateGasRaw = useCallback((value: string) => {
    setGas(value);
    const raw = normalizeRawInteger(value);
    if (raw === null) {
      setGasError("Enter a valid raw gas amount.");
      return;
    }
    if (raw && !Number.isSafeInteger(Number(raw))) {
      setGasError("Gas value is too large.");
      return;
    }
    setGasTgas(raw ? rawToDecimal(raw, TGAS_DECIMALS) : "");
    setGasError(raw && raw !== "0" ? null : "Enter a gas amount greater than 0.");
  }, []);

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

  useEffect(() => {
    if (!isTauriRuntime()) {
      setManualMode(true);
    }
  }, []);

  useEffect(() => {
    if (!loadError) {
      return;
    }
    devError("[SignTransaction] load keys error:", loadError);
    setError(loadError);
    setManualMode(true);
  }, [loadError]);

  useEffect(() => {
    if (manualMode) {
      return;
    }
    setSignerId(selectedAccountId);
    setPublicKey(selectedAccountPublicKey);
    setReceiverId((prev) => prev || selectedAccountId);
  }, [manualMode, selectedAccountId, selectedAccountPublicKey]);

  const keyEvaluations = useMemo(
    () =>
      keys.map((key) => ({
        key,
        compatibility: evaluateKeyCompatibility(
          key,
          actionType,
          receiverId,
          methodName,
          fnDeposit,
        ),
      })),
    [actionType, fnDeposit, keys, methodName, receiverId],
  );

  const compatibleKeys = useMemo(
    () => keyEvaluations.filter((k) => k.compatibility.compatible).map((k) => k.key),
    [keyEvaluations],
  );

  const visibleKeys = useMemo(
    () => (showAllKeys ? keyEvaluations.map((k) => k.key) : compatibleKeys),
    [compatibleKeys, keyEvaluations, showAllKeys],
  );
  const signableVisibleKeys = useMemo(
    () => visibleKeys.filter((key) => keyHasUsableSource(key)),
    [visibleKeys],
  );
  const allSignableKeyOptions = useMemo(
    () =>
      allSignableKeys.map((key) => ({
        id: signingKeyId(key),
        label: signingKeyOptionLabel(key),
      })),
    [allSignableKeys],
  );

  const selectedEvaluation = useMemo(
    () => keyEvaluations.find((k) => k.key.account_id === signerId && k.key.public_key === publicKey) ?? null,
    [keyEvaluations, publicKey, signerId],
  );
  const selectedLedgerSnapshot = useMemo(() => {
    if (!connectedLedgerSnapshot) {
      return null;
    }
    const matchesSelectedKey =
      signerId.trim() === connectedLedgerSnapshot.accountId &&
      publicKey.trim() === connectedLedgerSnapshot.publicKey;
    const matchesHardwareSelection =
      selectedCredentialSource === "hardware_wallet" &&
      (selectedAccountId || signerId).trim() === connectedLedgerSnapshot.accountId;
    return matchesSelectedKey || matchesHardwareSelection ? connectedLedgerSnapshot : null;
  }, [
    connectedLedgerSnapshot,
    publicKey,
    selectedAccountId,
    selectedCredentialSource,
    signerId,
  ]);
  const noCompatibleSelection =
    !manualMode && !showAllKeys && visibleKeys.length === 0 && !selectedLedgerSnapshot;
  const activeCompatibility = useMemo(() => {
    if (selectedEvaluation) {
      return selectedEvaluation.compatibility;
    }
    if (selectedLedgerSnapshot) {
      return evaluateKeyCompatibility(
        { permission: selectedLedgerSnapshot.permission },
        actionType,
        receiverId,
        methodName,
        fnDeposit,
      );
    }
    return null;
  }, [actionType, fnDeposit, methodName, receiverId, selectedEvaluation, selectedLedgerSnapshot]);
  const effectiveCredentialSource = useMemo(
    () => {
      if (selectedEvaluation) {
        return resolveCredentialSource(selectedEvaluation.key, selectedCredentialSource);
      }
      if (selectedLedgerSnapshot && selectedCredentialSource === "hardware_wallet") {
        return "hardware_wallet";
      }
      return selectedCredentialSource;
    },
    [selectedCredentialSource, selectedEvaluation, selectedLedgerSnapshot],
  );
  const selectedKeychainImportRequired = useMemo(
    () =>
      effectiveCredentialSource === "nearxd_keychain" &&
      Boolean(selectedEvaluation?.key.nearxd_keychain_import_required),
    [effectiveCredentialSource, selectedEvaluation],
  );
  const sourceUpgradeKind = useMemo(
    () =>
      resolveSourceUpgradeKind(
        selectedEvaluation?.key,
        effectiveCredentialSource,
        canPersistSecureStore,
      ),
    [canPersistSecureStore, effectiveCredentialSource, selectedEvaluation],
  );

  useEffect(() => {
    if (!selectedEvaluation) {
      if (selectedLedgerSnapshot && selectedCredentialSource === "hardware_wallet") {
        return;
      }
      if (!selectedAccountPublicKey) {
        setCredentialSource(null, false);
      }
      return;
    }
    if (
      selectedCredentialSource &&
      selectedEvaluation.key.available_sources.includes(selectedCredentialSource) &&
      resolveCredentialSource(selectedEvaluation.key, selectedCredentialSource) ===
        selectedCredentialSource
    ) {
      return;
    }
    setCredentialSource(resolveCredentialSource(selectedEvaluation.key, selectedCredentialSource));
  }, [selectedAccountPublicKey, selectedCredentialSource, selectedEvaluation, selectedLedgerSnapshot, setCredentialSource]);

  const compatibilityByKeyId = useMemo(() => {
    const map = new Map<string, KeyCompatibility>();
    for (const item of keyEvaluations) {
      map.set(signingKeyId(item.key), item.compatibility);
    }
    return map;
  }, [keyEvaluations]);

  const keyBadge = useCallback(
    (key: SigningKeyEntry): AccountPickerKeyBadge | null => {
      const compatibility = compatibilityByKeyId.get(signingKeyId(key));
      if (!compatibility || compatibility.compatible) {
        return null;
      }

      if (compatibility.risky) {
        return {
          text: "Deposit risk",
          tone: "warn",
          title: compatibility.reason ?? undefined,
        };
      }

      const reason = compatibility.reason ?? "Incompatible";
      if (reason.startsWith("Function-call keys cannot authorize Transfer")) {
        return { text: "No transfer", tone: "danger", title: reason };
      }
      if (reason.startsWith("Receiver must be")) {
        return { text: "Receiver mismatch", tone: "danger", title: reason };
      }
      if (reason.startsWith("Method is required")) {
        return { text: "Method required", tone: "danger", title: reason };
      }
      if (reason.startsWith("Method not permitted")) {
        return { text: "Method mismatch", tone: "danger", title: reason };
      }
      if (reason.startsWith("Unknown on-chain access permission")) {
        return { text: "Permission unknown", tone: "warn", title: reason };
      }
      return { text: "Incompatible", tone: "danger", title: reason };
    },
    [compatibilityByKeyId],
  );

  const incompatibleCount = useMemo(
    () => keyEvaluations.filter((k) => !k.compatibility.compatible).length,
    [keyEvaluations],
  );

  const argsState = useMemo(
    () => formatArgsPreview(argsJson),
    [argsJson],
  );
  const formValidationMessage = useMemo(() => {
    if (!receiverId.trim()) {
      return "Receiver is required.";
    }
    if (manualMode) {
      if (!signerId.trim() || !publicKey.trim()) {
        return "Enter both signer account and public key.";
      }
      return null;
    }
    if (noCompatibleSelection) {
      return "Choose a compatible signer key.";
    }
    if (!selectedEvaluation && !selectedLedgerSnapshot) {
      return "Select a signer key.";
    }
    if (!effectiveCredentialSource) {
      return "Source needed.";
    }
    if (selectedKeychainImportRequired) {
      return "Put this key in fingerprint-protected Keychain before signing with Keychain.";
    }
    if (activeCompatibility && !activeCompatibility.compatible) {
      return activeCompatibility.reason ?? "Selected key is not compatible.";
    }
    if (actionType === "Transfer") {
      return depositError;
    }
    if (!methodName.trim()) {
      return "Method name is required.";
    }
    return argsState.error ?? gasError ?? fnDepositError;
  }, [
    actionType,
    argsState.error,
    depositError,
    fnDepositError,
    gasError,
    manualMode,
    methodName,
    noCompatibleSelection,
    publicKey,
    receiverId,
    activeCompatibility,
    effectiveCredentialSource,
    selectedKeychainImportRequired,
    selectedEvaluation,
    selectedLedgerSnapshot,
    signerId,
  ]);

  useEffect(() => {
    if (manualMode || visibleKeys.length === 0 || selectedLedgerSnapshot) return;
    const stillVisible = visibleKeys.some(
      (k) => k.account_id === signerId && k.public_key === publicKey,
    );
    if (stillVisible) return;

    const fallback =
      preferSigningKey(visibleKeys, {
        accountId: lastAccountId,
        publicKey: lastPublicKey,
      }) ?? visibleKeys[0];

    if (fallback) {
      selectSignerKey(fallback);
      setReceiverId((prev) => prev || fallback.account_id);
    }
  }, [
    lastAccountId,
    lastPublicKey,
    manualMode,
    publicKey,
    selectedLedgerSnapshot,
    selectSignerKey,
    signerId,
    visibleKeys,
  ]);

  function selectKey(entry: SigningKeyEntry) {
    selectSignerKey(entry);
    setReceiverId((prev) => prev || entry.account_id);
    setSignResult(null);
    setBroadcastResult(null);
    setError(null);
    setHardwareError(null);
    setLedgerNotice(null);
  }

  const handleImport = useCallback(
    async (opts?: {
      accountId?: string;
      publicKey?: string;
      sourceHints?: CredentialSource[];
      keyId?: string;
      overwrite?: boolean;
    }) => {
      if (!canPersistSecureStore) {
        setError("OS secure storage is unavailable on this platform.");
        return;
      }
      setImporting(true);
      setImportingKeyId(opts?.keyId);
      setImportingAccountId(opts?.accountId);
      setImportMsg(null);
      setError(null);
      setHardwareError(null);

      const sourceHints =
        opts?.sourceHints?.filter(
          (s) => s !== "nearxd_keychain" && s !== "hardware_wallet",
        ) ?? importSources;
      const sources = sourceHints.length > 0 ? sourceHints : importSources;
      const params = buildImportParams(signingCapabilities, {
        network: networkId,
        account_id: opts?.accountId,
        public_key: opts?.publicKey,
        sources,
        overwrite: opts?.overwrite,
      });
      addDebug(`import_near_signing_keys request: ${JSON.stringify(params)}`);

      try {
        const result = await importNearSigningKeys(params);
        addDebug(`import_near_signing_keys response: ${JSON.stringify(result, null, 2)}`);
        const count = result.imported_count;
        const targetResult =
          opts?.accountId && opts?.publicKey
            ? [...(result.imported ?? []), ...(result.skipped ?? [])].find(
                (row) =>
                  row.account_id === opts.accountId && row.public_key === opts.publicKey,
              )
            : null;
        const keychainReadyForSelectedKey =
          !opts?.accountId ||
          !opts?.publicKey ||
          signingCapabilities?.platform !== "macos" ||
          targetResult?.keychain_protection === "biometry_current_set";
        if (result.failed && result.failed.length > 0) {
          addDebug(`import failures: ${JSON.stringify(result.failed, null, 2)}`);
        }
        if (result.skipped && result.skipped.length > 0) {
          addDebug(`import skipped: ${JSON.stringify(result.skipped, null, 2)}`);
        }
        if (opts?.accountId && opts?.publicKey && !keychainReadyForSelectedKey) {
          const fallbackSource =
            sources.find(
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
        const reloadAccountId = opts?.accountId ?? (selectedAccountId || undefined);
        await loadKeys(reloadAccountId);
        if (opts?.accountId && opts?.publicKey && keychainReadyForSelectedKey) {
          setCredentialSource("nearxd_keychain", true, {
            accountId: opts.accountId,
            publicKey: opts.publicKey,
          });
        } else if (opts?.accountId && opts?.publicKey) {
          const fallbackSource =
            sources.find(
              (source) =>
                source !== "nearxd_keychain" && source !== "hardware_wallet",
            ) ?? null;
          setCredentialSource(fallbackSource, true, {
            accountId: opts.accountId,
            publicKey: opts.publicKey,
          });
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        addDebug(`import_near_signing_keys ERROR: ${msg}`);
        setError(msg);
      } finally {
        setImporting(false);
        setImportingKeyId(undefined);
        setImportingAccountId(undefined);
      }
    },
    [
      addDebug,
      canPersistSecureStore,
      importSources,
      loadKeys,
      selectedAccountId,
      setCredentialSource,
      signingCapabilities,
      storageLabel,
    ],
  );

  const handleUpgradeToKeychain = useCallback(async () => {
    if (!selectedEvaluation) return;
    const key = selectedEvaluation.key;
    if (sourceUpgradeKind === "repair") {
      setImporting(true);
      setImportingKeyId(signingKeyId(key));
      setImportingAccountId(key.account_id);
      setImportMsg(null);
      setError(null);
      try {
        const result = await reprotectNearSigningKey({
          network: networkId,
          account_id: key.account_id,
          public_key: key.public_key,
          reason:
            "NEARx needs your approval to enable fingerprint-protected Keychain signing for this signer.",
        });
        addDebug(`reprotect_near_signing_key response: ${JSON.stringify(result, null, 2)}`);
        setImportMsg("Enabled fingerprint-protected Keychain for the selected key.");
        await loadKeys(key.account_id);
        setCredentialSource("nearxd_keychain", true, {
          accountId: key.account_id,
          publicKey: key.public_key,
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        addDebug(`reprotect_near_signing_key ERROR: ${msg}`);
        setError(msg);
      } finally {
        setImporting(false);
        setImportingKeyId(undefined);
        setImportingAccountId(undefined);
      }
      return;
    }

    const sourceHints = key.available_sources.filter(
      (s) => s !== "nearxd_keychain" && s !== "hardware_wallet",
    );
    await handleImport({
      accountId: key.account_id,
      publicKey: key.public_key,
      sourceHints,
      keyId: signingKeyId(key),
      overwrite: Boolean(key.nearxd_keychain_import_required),
    });
  }, [
    addDebug,
    handleImport,
    loadKeys,
    selectedEvaluation,
    setCredentialSource,
    sourceUpgradeKind,
  ]);

  const handleConnectLedger = useCallback(async () => {
    if (!isTauriRuntime()) return;
    const accountId = selectedLedgerAccountId;
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
      addDebug(`connect_hardware_wallet response: ${JSON.stringify(connected, null, 2)}`);
      const snapshot = snapshotFromConnectResult(connected);
      setLedgerConnection(connected);
      setConnectedLedgerSnapshot(snapshot);
      const successMessage =
        connected.account_binding === "implicit_account"
          ? `Ledger connected for implicit account ${connected.account_id} on path ${connected.derivation_path}.`
          : `Ledger key connected for ${connected.account_id} on path ${connected.derivation_path}.`;
      setHardwareMsg(successMessage);
      setLedgerNotice(successMessage);
      setManualMode(false);
      setSignerSelection({
        accountId: connected.account_id,
        publicKey: connected.public_key,
        credentialSource: "hardware_wallet",
      });
      setReceiverId((prev) => prev || connected.account_id);
      setShowManageSigner(false);
      await loadKeys(connected.account_id);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      addDebug(`connect_hardware_wallet ERROR: ${msg}`);
      const friendly = ledgerHardwareErrorMessage(msg);
      setHardwareError(friendly);
      setError(friendly);
    } finally {
      setLedgerConnecting(false);
    }
  }, [
    addDebug,
    canConnectLedger,
    ledgerTiedToSelectedAccount,
    ledgerDerivationPath,
    loadKeys,
    selectedLedgerAccountId,
    setSignerSelection,
  ]);

  const switchAwayFromHardware = useCallback(() => {
    if (!selectedEvaluation) return;
    const fallback = fallbackLocalSource(selectedEvaluation.key);
    if (fallback) {
      setCredentialSource(fallback);
      setHardwareError(null);
      setError(null);
      setLedgerNotice(null);
      return;
    }
    setHardwareError("No other local source is available for this key.");
  }, [selectedEvaluation]);

  const selectedSourceLabel = useMemo(() => {
    if (effectiveCredentialSource) {
      return signerSourceLabel(
        effectiveCredentialSource,
        selectedEvaluation?.key,
        signingCapabilities,
      );
    }
    return manualMode ? "Manual entry" : "Not selected";
  }, [effectiveCredentialSource, manualMode, selectedEvaluation, signingCapabilities]);
  const signerSourceOptions = useMemo(() => {
    if (selectedEvaluation) {
      return signerSourceOptionsForKey(selectedEvaluation.key, signingCapabilities);
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
  }, [effectiveCredentialSource, selectedEvaluation, selectedLedgerSnapshot, signingCapabilities]);
  const selectedWeakSourceWarning = useMemo(() => {
    if (
      !selectedEvaluation ||
      signingCapabilities?.platform !== "macos" ||
      !selectedEvaluation.key.available_sources.includes("nearxd_keychain")
    ) {
      return null;
    }
    if (
      effectiveCredentialSource === "legacy_file" ||
      effectiveCredentialSource === "near_cli_secure"
    ) {
      if (selectedEvaluation.key.nearxd_keychain_import_required) {
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
  }, [effectiveCredentialSource, selectedEvaluation, signingCapabilities]);
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

  const selectedLedgerPath = useMemo(
    () =>
      selectedEvaluation?.key.hardware_wallet?.derivation_path ??
      selectedLedgerSnapshot?.derivationPath ??
      ledgerConnection?.derivation_path ??
      null,
    [
      ledgerConnection?.derivation_path,
      selectedEvaluation?.key.hardware_wallet?.derivation_path,
      selectedLedgerSnapshot?.derivationPath,
    ],
  );

  const signerSummaryStatus = useMemo(() => {
    return resolveSignerSummaryStatus({
      hardwareError,
      error,
      neutralMessage:
        selectedEvaluation ||
        selectedLedgerSnapshot ||
        (manualMode && signerId.trim() && publicKey.trim())
          ? null
          : "Select a key or connect Ledger to prepare a signer.",
      selectionRequiredMessage:
        manualMode && (!signerId.trim() || !publicKey.trim())
          ? "Enter a signer account and public key, or choose one from Manage signer."
          : noCompatibleSelection
            ? "Adjust the signer or transaction details to find a compatible key."
            : !(selectedEvaluation || selectedLedgerSnapshot) && !manualMode
              ? "Select a signer key."
              : null,
      selectionRequiredLabel:
        noCompatibleSelection ? "Choose compatible key" : "Signer required",
      sourceNeededMessage:
        (selectedEvaluation || selectedLedgerSnapshot) && !effectiveCredentialSource
          ? "Choose a signer with a local source, or import/connect one in Manage signer."
          : null,
      incompatibleLabel: selectedKeychainImportRequired
        ? "Fingerprint required"
        : "Choose compatible key",
      incompatibleMessage:
        selectedKeychainImportRequired
          ? "Keychain signing is blocked until this key is in fingerprint-protected Keychain."
          : activeCompatibility && !activeCompatibility.compatible
          ? (
              activeCompatibility.reason ??
              "Selected signer needs attention before it can sign this transaction."
            )
          : null,
      advisoryLabel: selectedWeakSourceLabel,
      advisoryMessage: selectedWeakSourceWarning,
      readyLabel: "Ready to sign",
      readyMessage:
        ledgerNotice ??
        importMsg ??
        "Signer is ready. Use Manage signer only if you want to switch keys, import, or connect Ledger.",
    });
  }, [
    error,
    hardwareError,
    importMsg,
    ledgerNotice,
    manualMode,
    noCompatibleSelection,
    publicKey,
    activeCompatibility,
    effectiveCredentialSource,
    selectedKeychainImportRequired,
    selectedEvaluation,
    selectedLedgerSnapshot,
    selectedWeakSourceLabel,
    selectedWeakSourceWarning,
    signerId,
  ]);

  const openConfirmModal = useCallback(() => {
    if (formValidationMessage) {
      setError(formValidationMessage);
      return;
    }
    setError(null);
    setBroadcastError(null);
    setShowConfirmModal(true);
  }, [formValidationMessage]);

  const broadcastSignedTransaction = useCallback(
    async (signedTransactionBase64: string) => {
      setBroadcastResult(null);
      setBroadcastError(null);
      try {
        const result = await broadcastTransaction(signedTransactionBase64);
        addDebug(`broadcast_transaction success: ${JSON.stringify(result, null, 2)}`);
        setBroadcastResult(result);
        return true;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        addDebug(`broadcast_transaction ERROR: ${msg}`);
        setBroadcastError(msg);
        return false;
      }
    },
    [addDebug],
  );

  const handleConfirmSign = useCallback(async (broadcastAfterSign: boolean) => {
    setShowConfirmModal(false);
    setSignResult(null);
    setBroadcastResult(null);
    setBroadcastError(null);
    setHardwareError(null);
    setError(null);
    setPendingAction(broadcastAfterSign ? "sign_and_broadcast" : "sign");
    setLoading(true);
    try {
      addDebug(`viewAccessKey(${signerId}, ${publicKey})`);
      const ak = await viewAccessKey(signerId, publicKey);
      addDebug(`access key: nonce=${ak.nonce}, block_hash=${ak.block_hash}`);

      let actions;
      if (actionType === "Transfer") {
        actions = [{ type: "Transfer" as const, deposit }];
      } else {
        const argsBase64 = btoa(argsJson.trim());
        actions = [
          {
            type: "FunctionCall" as const,
            method_name: methodName,
            args: argsBase64,
            gas: Number(gas),
            deposit: fnDeposit,
          },
        ];
      }

      const signParams = {
        signer_id: signerId,
        signer_public_key: publicKey,
        credential_source: effectiveCredentialSource ?? undefined,
        receiver_id: receiverId,
        nonce: ak.nonce + 1,
        block_hash: ak.block_hash,
        actions,
        network: networkId,
        reason: `Sign ${actionType} to ${receiverId}`,
      };
      addDebug(`sign_transaction request: ${JSON.stringify(signParams, null, 2)}`);

      const result = await signTransaction(signParams);
      addDebug(
        `sign_transaction success: tx_hash=${result.tx_hash}, source=${result.credential_source ?? "unknown"}`,
      );
      setSignResult(result);
      if (broadcastAfterSign) {
        setPendingAction("broadcast");
        await broadcastSignedTransaction(result.signed_transaction_base64);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      addDebug(`sign_transaction ERROR: ${msg}`);
      const friendly =
        effectiveCredentialSource === "hardware_wallet"
          ? ledgerHardwareErrorMessage(msg)
          : msg;
      setError(friendly);
      if (
        effectiveCredentialSource === "hardware_wallet" &&
        (msg.startsWith("ERR_HARDWARE_") || msg.startsWith("ERR_UNAVAILABLE"))
      ) {
        setShowLedgerPanel(true);
        setHardwareError(friendly);
      }
    } finally {
      setPendingAction(null);
      setLoading(false);
    }
  }, [
    actionType,
    addDebug,
    argsJson,
    deposit,
    formValidationMessage,
    fnDeposit,
    gas,
    methodName,
    publicKey,
    receiverId,
    effectiveCredentialSource,
    signerId,
    broadcastSignedTransaction,
  ]);

  const handleBroadcast = useCallback(async () => {
    if (!signResult) return;
    setError(null);
    setPendingAction("broadcast");
    setLoading(true);
    try {
      await broadcastSignedTransaction(signResult.signed_transaction_base64);
    } finally {
      setPendingAction(null);
      setLoading(false);
    }
  }, [broadcastSignedTransaction, signResult]);

  const broadcastSummary = useMemo(() => {
    if (!signResult) {
      return null;
    }
    if (broadcastResult !== null) {
      return summarizeBroadcastResult(broadcastResult, signResult.tx_hash);
    }
    if (broadcastError) {
      return {
        txHash: signResult.tx_hash,
        success: false,
        statusLabel: "FAILURE",
      };
    }
    return null;
  }, [broadcastError, broadcastResult, signResult]);

  const signerSummaryItems = useMemo(
    () => [
      {
        label: "Account",
        value: (
          <span className="font-mono">
            {signerId || selectedAccountId || <span className="text-gray-400">Not selected</span>}
          </span>
        ),
      },
      {
        label: "Key",
        value: selectedEvaluation ? (
          <div className="min-w-0">
            {selectedEvaluation.key.label?.trim() ? (
              <div
                className="truncate text-sm font-medium text-gray-900"
                title={selectedEvaluation.key.label ?? undefined}
              >
                {selectedEvaluation.key.label}
              </div>
            ) : null}
            <div
              className="truncate font-mono text-sm text-gray-600"
              title={selectedEvaluation.key.public_key}
            >
              {shortPublicKey(selectedEvaluation.key.public_key)}
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
        ) : manualMode && publicKey.trim() ? (
          <span className="font-mono">{shortPublicKey(publicKey)}</span>
        ) : (
          <span className="text-gray-400">Choose a signer key</span>
        ),
      },
      {
        label: "Permission",
        value: selectedEvaluation ? (
          permissionLabel(selectedEvaluation.key)
        ) : selectedLedgerSnapshot ? (
          permissionLabel({ permission: selectedLedgerSnapshot.permission })
        ) : manualMode && publicKey.trim() ? (
          "Not known"
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
    ],
    [
      manualMode,
      publicKey,
      selectedAccountId,
      selectedEvaluation,
      selectedLedgerSnapshot,
      selectedLedgerPath,
      selectedSourceLabel,
      signerId,
    ],
  );

  const confirmationItems = useMemo(
    () => [
      {
        label: "Signer account",
        value: (
          <span className="font-mono">
            {signerId || <span className="text-gray-400">Not selected</span>}
          </span>
        ),
      },
      {
        label: "Key",
        value: selectedEvaluation ? (
          <div className="min-w-0">
            {selectedEvaluation.key.label?.trim() ? (
              <div className="text-sm font-medium text-gray-900">{selectedEvaluation.key.label}</div>
            ) : null}
            <div className="break-all font-mono text-sm text-gray-600">
              {selectedEvaluation.key.public_key}
            </div>
          </div>
        ) : selectedLedgerSnapshot ? (
          <div className="min-w-0">
            {selectedLedgerSnapshot.label?.trim() ? (
              <div className="text-sm font-medium text-gray-900">{selectedLedgerSnapshot.label}</div>
            ) : null}
            <div className="break-all font-mono text-sm text-gray-600">
              {selectedLedgerSnapshot.publicKey}
            </div>
          </div>
        ) : publicKey.trim() ? (
          <span className="break-all font-mono">{publicKey}</span>
        ) : (
          <span className="text-gray-400">Not selected</span>
        ),
      },
      {
        label: "Permission",
        value: selectedEvaluation ? (
          permissionLabel(selectedEvaluation.key)
        ) : selectedLedgerSnapshot ? (
          permissionLabel({ permission: selectedLedgerSnapshot.permission })
        ) : publicKey.trim() ? (
          "Not known"
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
      {
        label: "Receiver",
        value: receiverId ? (
          <span className="font-mono">{receiverId}</span>
        ) : (
          <span className="text-gray-400">Not entered</span>
        ),
      },
      {
        label: "Action",
        value: actionType,
      },
      {
        label: "Deposit",
        value:
          actionType === "Transfer"
            ? `${depositNear || "0"} NEAR • ${deposit || "0"} yocto`
            : `${fnDepositNear || "0"} NEAR • ${fnDeposit || "0"} yocto`,
      },
      ...(actionType === "FunctionCall"
        ? [
            {
              label: "Method",
              value: methodName || <span className="text-gray-400">Not entered</span>,
            },
            {
              label: "Gas",
              value: `${gasTgas || "0"} Tgas • ${gas || "0"} raw gas`,
            },
          ]
        : []),
    ],
    [
      actionType,
      deposit,
      depositNear,
      fnDeposit,
      fnDepositNear,
      gas,
      gasTgas,
      methodName,
      publicKey,
      receiverId,
      selectedEvaluation,
      selectedLedgerSnapshot,
      selectedLedgerPath,
      selectedSourceLabel,
      signerId,
    ],
  );
  const accountKeyTab = (
    <div className="space-y-4 px-4 py-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="text-xs font-medium uppercase tracking-wide text-gray-500">
            Account and key
          </div>
          <div className="mt-1 text-sm text-gray-500">
            Choose the account and key that will sign this transaction.
          </div>
        </div>
        {!manualMode && keys.length > 0 ? (
          <button
            type="button"
            onClick={() => setManualMode(true)}
            className="rounded border border-gray-200 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-50"
          >
            Manual
          </button>
        ) : null}
        {manualMode && keys.length > 0 ? (
          <button
            type="button"
            onClick={() => {
              setManualMode(false);
              const fallback =
                signableVisibleKeys.find(
                  (k) =>
                    k.account_id === (lastAccountId ?? "") &&
                    k.public_key === (lastPublicKey ?? ""),
                ) ??
                signableVisibleKeys[0] ??
                visibleKeys.find(
                  (k) =>
                    k.account_id === (lastAccountId ?? "") &&
                    k.public_key === (lastPublicKey ?? ""),
                ) ?? visibleKeys[0];
              if (fallback) {
                selectKey(fallback);
              }
            }}
            className="rounded border border-gray-200 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-50"
          >
            Use key list
          </button>
        ) : null}
      </div>

      <div className="space-y-3">
        {!manualMode ? (
          <>
            {isTauriRuntime() ? (
              <label className="block text-sm text-gray-700">
                <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
                  Account
                </span>
                <select
                  value={selectedAccountId}
                  onChange={(e) => {
                    const nextAccountId = e.target.value;
                    void selectSignerAccount(nextAccountId);
                  }}
                  className={`${toolbarSelectClass} w-full min-w-0`}
                  disabled={accountsLoading || signerAccountOptions.length === 0}
                >
                  {signerAccountOptions.length === 0 ? (
                    <option value="">No accounts</option>
                  ) : (
                    signerAccountOptions.map((entry) => (
                      <option key={entry.account_id} value={entry.account_id}>
                        {entry.label}
                      </option>
                    ))
                  )}
                </select>
              </label>
            ) : null}
	            {allSignableKeyOptions.length > 0 ? (
	              <label className="block text-sm text-gray-700">
	                <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
	                  Key
	                </span>
	                <select
	                  value={selectedKeyId}
	                  onChange={(e) => {
	                    const next = allSignableKeys.find(
	                      (key) => signingKeyId(key) === e.target.value,
	                    );
	                    if (next) {
	                      selectKey(next);
	                    }
	                  }}
	                  className={`${toolbarSelectClass} w-full min-w-0`}
	                >
	                  {allSignableKeyOptions.map((option) => (
	                    <option key={option.id} value={option.id}>
	                      {option.label}
	                    </option>
	                  ))}
	                </select>
	              </label>
	            ) : null}
	            {visibleKeys.length > 0 && allSignableKeyOptions.length === 0 ? (
	              <div className="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700">
	                No local signing sources are available for this account. Import a software key or
	                connect Ledger below.
	              </div>
	            ) : null}
	          </>
	        ) : (
          <>
            <div className="text-sm text-gray-500">
              Enter a signer account and public key directly.
            </div>
            <label className="block text-sm text-gray-700">
              <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
                Account
              </span>
              <input
                className={inputClass}
                value={signerId}
                onChange={(e) => setSignerId(e.target.value)}
                placeholder="alice.near"
              />
            </label>
            <label className="block text-sm text-gray-700">
              <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-gray-500">
                Public key
              </span>
              <input
                className={inputClass}
                value={publicKey}
                onChange={(e) => setPublicKey(e.target.value)}
                placeholder="ed25519:..."
              />
            </label>
          </>
        )}
      </div>

      {selectedEvaluation && selectedEvaluation.key.available_sources.length > 1 ? (
        <div className="border-t border-gray-100 pt-4">
          <div className="mb-2 text-xs font-medium uppercase tracking-wide text-gray-500">
            Signing source
          </div>
          <div className="flex flex-wrap gap-1.5">
            {selectedEvaluation.key.available_sources.map((source) => {
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
                  {signerSourceLabel(source, selectedEvaluation.key, signingCapabilities)}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}

      {selectedEvaluation ? (
        <div className="border-t border-gray-100 pt-4">
          <SigningKeyLabelEditor
            entry={selectedEvaluation.key}
            network={networkId}
            disabled={accountsLoading || importing || ledgerConnecting}
            onSaved={() => loadKeys(selectedAccountId || undefined)}
          />
        </div>
      ) : null}

      {selectedEvaluation && !selectedEvaluation.compatibility.compatible && showAllKeys ? (
        <div className="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700">
          {selectedEvaluation.compatibility.reason}
        </div>
      ) : null}

      <div className="rounded-lg border border-gray-200 bg-surface/60">
        <div className="border-b border-gray-100 px-4 py-2 text-xs font-medium uppercase tracking-wide text-gray-500">
          Available keys
        </div>
        {accountsLoading ? (
          <div className="px-4 py-3 text-gray-500">Loading signer keys...</div>
        ) : !manualMode && visibleKeys.length > 0 ? (
          <AccountPicker
            keys={visibleKeys}
            selectedAccountId={selectedAccountId}
            selectedPublicKey={selectedAccountPublicKey}
            onSelect={selectKey}
            onImport={(entry) =>
              handleImport({
                accountId: entry.account_id,
                publicKey: entry.public_key,
                sourceHints: entry.available_sources,
                keyId: `${entry.account_id}:${entry.public_key}`,
              })
            }
            keyBadge={showAllKeys ? keyBadge : undefined}
            importing={importing}
            importingKeyId={importingKeyId}
            signingCapabilities={signingCapabilities}
            isStarred={isStarred}
            onToggleStar={toggleStar}
          />
        ) : !manualMode && keys.length > 0 ? (
          <div className="px-4 py-6 text-center text-amber-700">
            No compatible keys for this transaction. Enable "Show all keys" in the Import tab to override.
          </div>
        ) : isTauriRuntime() && keys.length === 0 ? (
          <div className="px-4 py-6 text-center">
            <p className="mb-3 text-gray-600">
              {selectedAccountId ? (
                <>
                  No signing keys found for <strong>{selectedAccountId}</strong> on{" "}
                  <strong>{networkId}</strong>.
                </>
              ) : (
                <>
                  No signing accounts found for <strong>{networkId}</strong>.
                </>
              )}
            </p>
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
        ) : (
          <div className="space-y-3 px-4 py-4">
            <div className="text-sm text-gray-500">
              Manual mode is enabled. Enter a signer above, or switch back to the key list.
            </div>
          </div>
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
          Import from the file system or OS secret store into {storageLabel.toLowerCase()}.
        </div>
      </div>

      <label className="inline-flex items-center gap-2 text-sm text-gray-700">
        <input
          type="checkbox"
          checked={showAllKeys}
          onChange={(e) => setShowAllKeys(e.target.checked)}
          className="rounded border-gray-300"
        />
        Show all keys
      </label>

      {!showAllKeys && incompatibleCount > 0 ? (
        <div className="text-sm text-gray-500">
          Filtered out {incompatibleCount} incompatible key(s)
        </div>
      ) : null}

      <div>
        <div className="mb-2 text-xs font-medium uppercase tracking-wide text-gray-500">
          Import sources
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
      </div>

      {secureStoreName ? (
        <div className="text-sm text-gray-500">
          {storageLabel}: {secureStoreName}
        </div>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => handleImport()}
          disabled={importing || importSources.length === 0 || !canPersistSecureStore}
          className="inline-flex items-center gap-1 rounded bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
        >
          <Download size={14} />
          {importing && !importingAccountId ? "Importing..." : "Import all"}
        </button>
        {signerId ? (
          <button
            type="button"
            onClick={() => handleImport({ accountId: signerId })}
            disabled={importing || importSources.length === 0 || !canPersistSecureStore}
            className="inline-flex items-center gap-1 rounded border border-gray-200 px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50"
          >
            {importing && importingAccountId === signerId && !importingKeyId
              ? "Importing..."
              : "Import selected account"}
          </button>
        ) : null}
        {selectedEvaluation ? (
          <button
            type="button"
            onClick={() =>
              handleImport({
                accountId: selectedEvaluation.key.account_id,
                publicKey: selectedEvaluation.key.public_key,
                sourceHints: selectedEvaluation.key.available_sources,
                keyId: `${selectedEvaluation.key.account_id}:${selectedEvaluation.key.public_key}`,
                overwrite: Boolean(selectedEvaluation.key.nearxd_keychain_import_required),
              })
            }
            disabled={importing || importSources.length === 0 || !canPersistSecureStore}
            className="inline-flex items-center gap-1 rounded border border-gray-200 px-3 py-2 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50"
          >
            {importing && importingKeyId === signingKeyId(selectedEvaluation.key)
              ? "Importing..."
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
          derivationPath={ledgerDerivationPath}
          onDerivationPathChange={setLedgerDerivationPath}
          tiedToSelectedAccount={ledgerTiedToSelectedAccount}
          onTiedToSelectedAccountChange={(value) => {
            setLedgerTiedToSelectedAccount(value);
            setHardwareError(null);
            setHardwareMsg(null);
          }}
          selectedAccountLabel={selectedLedgerAccountId || null}
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
      <h1 className="mb-4 text-xl font-bold">Sign Transaction</h1>

      <div className="mb-4 grid gap-4 lg:grid-cols-[minmax(0,1.22fr)_minmax(320px,0.78fr)]">
        <div className="min-w-0">
          <div className="rounded-lg border border-gray-200 bg-surface text-sm">
            <div className="border-b border-gray-100 px-4 py-2">
              <h2 className="text-xs font-medium uppercase text-gray-500">Transaction</h2>
            </div>

            <div className={rowClass}>
              <span className={labelClass}>Receiver</span>
              <input
                className={inputClass}
                value={receiverId}
                onChange={(e) => setReceiverId(e.target.value)}
                placeholder="bob.near"
              />
            </div>

            <div className={rowClass}>
              <span className={labelClass}>Action</span>
              <select
                className={inputClass}
                value={actionType}
                onChange={(e) => setActionType(e.target.value as ActionType)}
              >
                <option value="Transfer">Transfer</option>
                <option value="FunctionCall">Function Call</option>
              </select>
            </div>

            {actionType === "Transfer" && (
              <DualUnitInput
                label="Amount"
                primaryLabel="NEAR"
                secondaryLabel="yocto"
                primaryValue={depositNear}
                secondaryValue={deposit}
                onPrimaryChange={updateTransferNear}
                onSecondaryChange={updateTransferYocto}
                primaryPlaceholder="1"
                secondaryPlaceholder="1000000000000000000000000"
                error={depositError}
              />
            )}

            {actionType === "FunctionCall" && (
              <>
                <div className={rowClass}>
                  <span className={labelClass}>Method</span>
                  <input
                    className={inputClass}
                    value={methodName}
                    onChange={(e) => setMethodName(e.target.value)}
                    placeholder="ft_transfer"
                  />
                </div>
                <div className="border-b border-gray-100 px-4 py-2.5">
                  <div className="flex items-baseline gap-3">
                    <span className={labelClass}>Args (JSON)</span>
                    <textarea
                      className={inputClass + " h-20 font-mono"}
                      value={argsJson}
                      onChange={(e) => setArgsJson(e.target.value)}
                      placeholder='{"receiver_id": "bob.near", "amount": "1000"}'
                    />
                  </div>
                </div>
                <DualUnitInput
                  label="Gas"
                  primaryLabel="Tgas"
                  secondaryLabel="raw gas"
                  primaryValue={gasTgas}
                  secondaryValue={gas}
                  onPrimaryChange={updateGasTgas}
                  onSecondaryChange={updateGasRaw}
                  primaryPlaceholder="30"
                  secondaryPlaceholder="30000000000000"
                  error={gasError}
                />
                <DualUnitInput
                  label="Deposit"
                  primaryLabel="NEAR"
                  secondaryLabel="yocto"
                  primaryValue={fnDepositNear}
                  secondaryValue={fnDeposit}
                  onPrimaryChange={updateFunctionDepositNear}
                  onSecondaryChange={updateFunctionDepositYocto}
                  primaryPlaceholder="0"
                  secondaryPlaceholder="0"
                  error={fnDepositError}
                />
              </>
            )}

            <div className="space-y-3 border-t border-gray-100 px-4 py-4">
              {error && (
                <div className="rounded border border-red-200 bg-rose-50 px-3 py-2.5 text-sm text-rose-700">
                  {error}
                </div>
              )}

              {actionType === "FunctionCall" && argsState.error && !error && (
                <div className="rounded border border-amber-200 bg-amber-50 px-3 py-2.5 text-sm text-amber-800">
                  {argsState.error}
                </div>
              )}

              <div className="flex flex-wrap items-center gap-3">
                <button
                  onClick={openConfirmModal}
                  disabled={loading || Boolean(formValidationMessage)}
                  className="rounded bg-blue-600 px-5 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Sign
                </button>
                {formValidationMessage ? (
                  <span className="text-sm text-gray-500">{formValidationMessage}</span>
                ) : (
                  <span className="text-sm text-gray-500">
                    Sign first. Broadcast stays optional afterward.
                  </span>
                )}
              </div>
            </div>
          </div>
        </div>

        <div className="min-w-0 lg:sticky lg:top-6 lg:self-start">
          <SignerSummaryCard
            title="Signer"
            statusLabel={signerSummaryStatus.label}
            statusTone={signerSummaryStatus.tone}
            message={signerSummaryStatus.message}
            controls={
              <SignerQuickSelectors
                account={{
                  label: "Account",
                  value: selectedAccountId,
                  options: signerAccountOptions.map((entry) => ({
                    value: entry.account_id,
                    label: entry.label,
                  })),
                  onChange: (value) => {
                    setShowManageSigner(false);
                    setManualMode(false);
                    void selectSignerAccount(value);
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
                      setManualMode(false);
                      selectKey(next);
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
                  action: isUpgradeEligible(selectedEvaluation?.key, effectiveCredentialSource, canPersistSecureStore)
                    ? {
                        label: upgradeButtonLabel(
                          signingCapabilities,
                          sourceUpgradeKind ?? "import",
                          Boolean(selectedEvaluation?.key.nearxd_keychain_import_required),
                        ),
                        onClick: () => void handleUpgradeToKeychain(),
                        disabled: importing,
                        loading: importing && importingKeyId === selectedKeyId,
                        loadingLabel: upgradeLoadingLabel(
                          sourceUpgradeKind ?? "import",
                          Boolean(selectedEvaluation?.key.nearxd_keychain_import_required),
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
          {(ledgerNotice || (selectedLedgerSnapshot && effectiveCredentialSource === "hardware_wallet")) && (
            <div className="rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-800">
              <div className="font-medium">Ledger connected</div>
              <div className="mt-1">
                {ledgerNotice ??
                  `Ready to sign with ${shortPublicKey(selectedLedgerSnapshot?.publicKey ?? publicKey)}.`}
              </div>
              {selectedLedgerSnapshot && effectiveCredentialSource === "hardware_wallet" ? (
                <div className="mt-3 flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => setReceiverId(selectedLedgerSnapshot.accountId)}
                    className="rounded border border-emerald-300 px-2.5 py-1 text-xs font-medium text-emerald-800 hover:bg-emerald-100"
                  >
                    Use signer as receiver
                  </button>
                  <button
                    type="button"
                    onClick={() => setActionType("Transfer")}
                    className="rounded border border-emerald-300 px-2.5 py-1 text-xs font-medium text-emerald-800 hover:bg-emerald-100"
                  >
                    Set transfer
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setActionType("Transfer");
                      updateTransferYocto("1");
                    }}
                    className="rounded border border-emerald-300 px-2.5 py-1 text-xs font-medium text-emerald-800 hover:bg-emerald-100"
                  >
                    Set 1 yocto
                  </button>
                </div>
              ) : null}
            </div>
          )}
        </div>
      </div>

      <ManageSignerPanel
        open={showManageSigner}
        onClose={() => setShowManageSigner(false)}
        activeTab={activeSignerTab}
        onTabChange={(tab) => {
          setActiveSignerTab(tab);
          if (tab === "ledger") {
            setShowLedgerPanel(true);
            setHardwareError(null);
            setHardwareMsg(null);
          }
        }}
      >
        {activeSignerTab === "account_key" ? accountKeyTab : null}
        {activeSignerTab === "import" ? importTab : null}
        {activeSignerTab === "ledger" ? ledgerTab : null}
      </ManageSignerPanel>

      <SignTransactionConfirmationModal
        open={showConfirmModal}
        onClose={() => setShowConfirmModal(false)}
        onSign={() => void handleConfirmSign(false)}
        onSignAndBroadcast={() => void handleConfirmSign(true)}
        confirming={loading}
        items={confirmationItems}
        argsPreview={actionType === "FunctionCall" ? argsState.preview : null}
      />

      {signResult && (
        <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-gray-100 px-4 py-2">
            <h2 className="text-xs font-medium uppercase text-gray-500">Signed Transaction</h2>
            <button
              onClick={handleBroadcast}
              disabled={loading}
              className="rounded bg-green-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {pendingAction === "broadcast" ? "Broadcasting..." : broadcastSummary ? "Broadcast again" : "Broadcast"}
            </button>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>TX Hash</span>
            <code className="min-w-0 flex-1 break-all text-xs">{signResult.tx_hash}</code>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>Signer</span>
            <code className="min-w-0 flex-1 break-all text-xs">{signResult.signer_id}</code>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>Public Key</span>
            <code className="min-w-0 flex-1 break-all text-xs">{signResult.public_key}</code>
          </div>
          {signResult.credential_source && (
            <div className={rowClass}>
              <span className={labelClass}>Source</span>
              <code className="min-w-0 flex-1 break-all text-xs">{signResult.credential_source}</code>
            </div>
          )}
          <div className="border-b border-gray-100 px-4 py-2.5 last:border-b-0">
            <details>
              <summary className="flex cursor-pointer items-baseline gap-3">
                <span className={labelClass}>Payload</span>
                <span className="text-xs text-gray-500 hover:text-gray-900">
                  Base64 ({signResult.signed_transaction_base64.length} chars)
                </span>
              </summary>
              <pre className="mt-2 overflow-x-auto break-all whitespace-pre-wrap rounded bg-gray-100 p-2.5 text-xs">
                {signResult.signed_transaction_base64}
              </pre>
            </details>
          </div>
        </div>
      )}

      {broadcastSummary && (
        <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="border-b border-gray-100 px-4 py-2">
            <h2
              className={`text-xs font-medium uppercase ${
                broadcastSummary.success === false ? "text-rose-600" : "text-green-600"
              }`}
            >
              {broadcastSummary.success === false ? "Broadcast Failed" : "Broadcast Result"}
            </h2>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>TX Hash</span>
            <code className="min-w-0 flex-1 break-all text-xs">{broadcastSummary.txHash}</code>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>Status</span>
            <span className="min-w-0 flex-1 break-all text-xs">{broadcastSummary.statusLabel}</span>
          </div>
          {broadcastError ? (
            <div className="border-b border-gray-100 px-4 py-3 text-sm text-rose-700">
              {broadcastError}
            </div>
          ) : null}
          {broadcastResult !== null ? (
            <div className="px-4 py-3">
              <details>
                <summary className="cursor-pointer text-xs font-medium uppercase tracking-wide text-gray-500">
                  Raw Result
                </summary>
                <pre className="mt-3 overflow-x-auto rounded bg-gray-100 p-2.5 text-xs">
                  {JSON.stringify(broadcastResult, null, 2)}
                </pre>
              </details>
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}
