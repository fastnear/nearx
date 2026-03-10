import { useState, useEffect, useCallback, useMemo } from "react";
import { Link } from "react-router-dom";
import { Loader2 } from "lucide-react";
import { viewAccessKey, broadcastTransaction } from "../api/rpc";
import {
  isTauriRuntime,
  signTransaction,
  pickWasmFile,
} from "../tauri/runtime";
import type {
  AccessPermission,
  SignTransactionResult,
  SigningKeyEntry,
} from "../tauri/runtime";
import { networkId } from "../config";
import { useAccountPrefs } from "../hooks/useAccountPrefs";
import useSignerSelection from "../hooks/useSignerSelection";
import SignerSummaryCard from "../components/SignerSummaryCard";
import SignerQuickSelectors from "../components/SignerQuickSelectors";
import CopyableValue from "../components/CopyableValue";
import DualUnitInput from "../components/DualUnitInput";
import SignTransactionConfirmationModal from "../components/SignTransactionConfirmationModal";
import {
  signingAccountOptionLabel,
  permissionLabel,
  signingKeyOptionLabel,
} from "../lib/hardwareWalletDisplay";
import { summarizeBroadcastResult } from "../lib/broadcastSummary";
import { resolveSignerSummaryStatus } from "../lib/signerSummaryStatus";
import {
  preferSigningKey,
  signingKeyId,
} from "../lib/signerSourceSelection";
import {
  ledgerHardwareErrorMessage,
} from "../lib/ledgerConnectionUi";
import { usePreferences } from "../hooks/usePreferences";

type ActionType = "Transfer" | "FunctionCall" | "DeployContract" | "CreateAccount" | "DeleteAccount" | "DeleteKey" | "AddKey" | "Stake";

type KeyCompatibility = {
  compatible: boolean;
  risky: boolean;
  reason: string | null;
};

const rowClass =
  "flex items-baseline gap-3 border-b border-gray-100 px-4 py-3 last:border-b-0";
const labelClass = "shrink-0 w-24 text-right text-gray-500";
const inputClass =
  "flex-1 min-w-0 rounded-md border border-gray-300 bg-surface px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20 focus:outline-none";
const DEV_LOG = import.meta.env.DEV;
const YOCTO_DECIMALS = 24;
const TGAS_DECIMALS = 12;
const SIGN_TIMEOUT_MS = 30_000;

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
    // Permission lookup failed (RPC unavailable) — allow signing anyway.
    // The chain will reject at broadcast time if the key lacks permission.
    return { compatible: true, risky: false, reason: null };
  }

  if (actionType !== "FunctionCall") {
    return {
      compatible: false,
      risky: false,
      reason: `${actionType} requires a full-access key.`,
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
    starredAccounts,
    lastAccountId,
    lastPublicKey,
    setLastAccount,
    setLastKey,
    sortAccounts,
    toggleStar,
  } = useAccountPrefs("sign");
  const [manualMode, setManualMode] = useState(false);
  const [showAllKeys] = useState(false);
  const [hardwareError, setHardwareError] = useState<string | null>(null);

  const [signerId, setSignerId] = useState("");
  const [publicKey, setPublicKey] = useState("");
  const [receiverId, setReceiverId] = useState("");
  const [createAccountPrefix, setCreateAccountPrefix] = useState("");
  const [createDepositNear, setCreateDepositNear] = useState("1");
  const [createDeposit, setCreateDeposit] = useState("1000000000000000000000000");
  const [createDepositError, setCreateDepositError] = useState<string | null>(null);
  const [actionType, setActionType] = useState<ActionType>("Transfer");
  const [showConfirmModal, setShowConfirmModal] = useState(false);

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

  // DeleteAccount
  const [beneficiaryId, setBeneficiaryId] = useState("");

  // DeleteKey
  const [deletePublicKey, setDeletePublicKey] = useState("");

  // AddKey
  const [addPublicKey, setAddPublicKey] = useState("");
  const [addKeyPermission, setAddKeyPermission] = useState<"FullAccess" | "FunctionCall">("FullAccess");
  const [addKeyAllowanceNear, setAddKeyAllowanceNear] = useState("");
  const [addKeyAllowance, setAddKeyAllowance] = useState("");
  const [addKeyAllowanceError, setAddKeyAllowanceError] = useState<string | null>(null);
  const [addKeyReceiverId, setAddKeyReceiverId] = useState("");
  const [addKeyMethodNames, setAddKeyMethodNames] = useState("");

  // Stake
  const [stakeAmountNear, setStakeAmountNear] = useState("");
  const [stakeAmount, setStakeAmount] = useState("");
  const [stakeAmountError, setStakeAmountError] = useState<string | null>(null);
  const [stakePublicKey, setStakePublicKey] = useState("");

  // DeployContract
  const [deployCode, setDeployCode] = useState("");
  const [deployFileName, setDeployFileName] = useState<string | null>(null);
  const [deployFileSize, setDeployFileSize] = useState<number | null>(null);
  const [deployCodeError, setDeployCodeError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);

  const { preferences, updatePreference } = usePreferences();

  const [signResult, setSignResult] = useState<SignTransactionResult | null>(null);
  const [broadcastResult, setBroadcastResult] = useState<unknown>(null);
  const [broadcastError, setBroadcastError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [pendingAction, setPendingAction] = useState<"sign" | "sign_and_broadcast" | "broadcast" | null>(null);
  const {
    accountsLoading,
    allSignableKeys,
    keys,
    loadError,
    selectAccount: selectSignerAccount,
    selectKey: selectSignerKey,
    selectedAccountId,
    selectedKeyId,
    selectedPublicKey: selectedAccountPublicKey,
    signingAccounts,
  } = useSignerSelection({
    network: networkId,
    initialAccountId: lastAccountId ?? "",
    lastAccountId,
    lastPublicKey,
    sortAccounts,
    setLastAccount,
    setLastKey,
  });

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

  const updateAddKeyAllowanceNear = useCallback((value: string) => {
    setAddKeyAllowanceNear(value);
    const raw = decimalToRaw(value, YOCTO_DECIMALS);
    if (raw === null) {
      setAddKeyAllowanceError("Enter a valid NEAR amount.");
      return;
    }
    setAddKeyAllowance(raw);
    setAddKeyAllowanceError(null);
  }, []);
  const updateAddKeyAllowanceYocto = useCallback((value: string) => {
    setAddKeyAllowance(value);
    const raw = normalizeRawInteger(value);
    if (raw === null) {
      setAddKeyAllowanceError("Enter a valid yocto amount.");
      return;
    }
    setAddKeyAllowanceNear(raw ? rawToDecimal(raw, YOCTO_DECIMALS) : "");
    setAddKeyAllowanceError(null);
  }, []);
  const updateStakeAmountNear = useCallback((value: string) => {
    setStakeAmountNear(value);
    const raw = decimalToRaw(value, YOCTO_DECIMALS);
    if (raw === null) {
      setStakeAmountError("Enter a valid NEAR amount.");
      return;
    }
    setStakeAmount(raw);
    setStakeAmountError(raw && raw !== "0" ? null : "Enter a stake amount greater than 0.");
  }, []);
  const updateStakeAmountYocto = useCallback((value: string) => {
    setStakeAmount(value);
    const raw = normalizeRawInteger(value);
    if (raw === null) {
      setStakeAmountError("Enter a valid yocto amount.");
      return;
    }
    setStakeAmountNear(raw ? rawToDecimal(raw, YOCTO_DECIMALS) : "");
    setStakeAmountError(raw && raw !== "0" ? null : "Enter a stake amount greater than 0.");
  }, []);

  const updateCreateDepositNear = useCallback((value: string) => {
    setCreateDepositNear(value);
    const raw = decimalToRaw(value, YOCTO_DECIMALS);
    if (raw === null) {
      setCreateDepositError("Enter a valid NEAR amount.");
      return;
    }
    setCreateDeposit(raw);
    setCreateDepositError(null);
  }, []);
  const updateCreateDepositYocto = useCallback((value: string) => {
    setCreateDeposit(value);
    const raw = normalizeRawInteger(value);
    if (raw === null) {
      setCreateDepositError("Enter a valid yocto amount.");
      return;
    }
    setCreateDepositNear(raw ? rawToDecimal(raw, YOCTO_DECIMALS) : "");
    setCreateDepositError(null);
  }, []);

  const handlePickWasm = useCallback(async () => {
    try {
      const result = await pickWasmFile(preferences.last_wasm_directory);
      if (!result) return;
      setDeployCode(result.code_base64);
      setDeployFileName(result.file_name);
      setDeployFileSize(result.file_size);
      setDeployCodeError(null);
      if (result.directory) {
        void updatePreference("last_wasm_directory", result.directory);
      }
    } catch (err) {
      setDeployCodeError(err instanceof Error ? err.message : String(err));
    }
  }, [preferences.last_wasm_directory, updatePreference]);

  const handleDeployDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    const file = e.dataTransfer.files[0];
    if (!file || !file.name.endsWith(".wasm")) {
      setDeployCodeError("Drop a .wasm file.");
      return;
    }
    setDeployFileName(file.name);
    setDeployFileSize(file.size);
    const reader = new FileReader();
    reader.onload = () => {
      const bytes = new Uint8Array(reader.result as ArrayBuffer);
      let binary = "";
      for (const b of bytes) binary += String.fromCharCode(b);
      setDeployCode(btoa(binary));
      setDeployCodeError(null);
    };
    reader.onerror = () => setDeployCodeError("Failed to read file.");
    reader.readAsArrayBuffer(file);
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
  const noCompatibleSelection =
    !manualMode && !showAllKeys && visibleKeys.length === 0;
  const activeCompatibility = useMemo(() => {
    if (selectedEvaluation) {
      return selectedEvaluation.compatibility;
    }
    return null;
  }, [selectedEvaluation]);
  const hasUsableSource = Boolean(
    selectedEvaluation && selectedEvaluation.key.available_sources.length > 0
  );

  const argsState = useMemo(
    () => formatArgsPreview(argsJson),
    [argsJson],
  );
  const receiverIsImplicit = actionType === "DeployContract"
    || actionType === "Stake"
    || actionType === "AddKey"
    || actionType === "DeleteKey"
    || actionType === "DeleteAccount";
  const effectiveReceiverId = receiverIsImplicit
    ? signerId
    : actionType === "CreateAccount"
      ? (createAccountPrefix.trim() ? `${createAccountPrefix.trim()}.${signerId}` : "")
      : receiverId;

  const formValidationMessage = useMemo(() => {
    if (!receiverIsImplicit && actionType !== "CreateAccount" && !receiverId.trim()) {
      return "Receiver is required.";
    }
    if (actionType === "CreateAccount" && !createAccountPrefix.trim()) {
      return "Sub-account name is required.";
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
    if (!selectedEvaluation) {
      return "Select a signer key.";
    }
    if (!hasUsableSource) {
      return "No usable source available for this key.";
    }
    if (activeCompatibility && !activeCompatibility.compatible) {
      return activeCompatibility.reason ?? "Selected key is not compatible.";
    }
    switch (actionType) {
      case "Transfer":
        return depositError;
      case "FunctionCall":
        if (!methodName.trim()) return "Method name is required.";
        return argsState.error ?? gasError ?? fnDepositError;
      case "DeployContract":
        return deployCodeError ?? (!deployCode ? "Select a WASM file to deploy." : null);
      case "CreateAccount":
        return createDepositError;
      case "DeleteAccount":
        if (!beneficiaryId.trim()) return "Beneficiary account is required.";
        return null;
      case "DeleteKey":
        if (!deletePublicKey.trim()) return "Public key is required.";
        return null;
      case "AddKey":
        if (!addPublicKey.trim()) return "Public key is required.";
        if (addKeyPermission === "FunctionCall" && !addKeyReceiverId.trim())
          return "Receiver account is required for FunctionCall permission.";
        return addKeyAllowanceError;
      case "Stake":
        if (!stakePublicKey.trim()) return "Validator public key is required.";
        return stakeAmountError;
    }
  }, [
    actionType,
    addKeyAllowanceError,
    addKeyPermission,
    addKeyReceiverId,
    addPublicKey,
    argsState.error,
    beneficiaryId,
    createAccountPrefix,
    createDepositError,
    deletePublicKey,
    deployCode,
    deployCodeError,
    depositError,
    fnDepositError,
    gasError,
    hasUsableSource,
    manualMode,
    methodName,
    noCompatibleSelection,
    publicKey,
    receiverId,
    receiverIsImplicit,
    activeCompatibility,
    selectedEvaluation,
    signerId,
    stakeAmountError,
    stakePublicKey,
  ]);

  useEffect(() => {
    if (manualMode || accountsLoading || visibleKeys.length === 0) return;
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
    accountsLoading,
    lastAccountId,
    lastPublicKey,
    manualMode,
    publicKey,
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
  }

  const selectedLedgerPath = useMemo(
    () =>
      selectedEvaluation?.key.hardware_wallet?.derivation_path ?? null,
    [selectedEvaluation?.key.hardware_wallet?.derivation_path],
  );

  const signerSummaryStatus = useMemo(() => {
    return resolveSignerSummaryStatus({
      hardwareError,
      error,
      neutralMessage:
        selectedEvaluation ||
        (manualMode && signerId.trim() && publicKey.trim())
          ? null
          : "Select a signing key.",
      readyLabel: "Ready to sign",
      readyMessage: "Signer is ready.",
    });
  }, [
    error,
    hardwareError,
    manualMode,
    publicKey,
    selectedEvaluation,
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
    const isHardwareKey = Boolean(selectedEvaluation?.key.hardware_wallet);
    try {
      addDebug(`viewAccessKey(${signerId}, ${publicKey})`);
      const ak = await viewAccessKey(signerId, publicKey);
      addDebug(`access key: nonce=${ak.nonce}, block_hash=${ak.block_hash}`);

      let actions;
      switch (actionType) {
        case "Transfer":
          actions = [{ type: "Transfer" as const, deposit }];
          break;
        case "FunctionCall": {
          const argsBase64 = btoa(argsJson.trim());
          actions = [{
            type: "FunctionCall" as const,
            method_name: methodName,
            args: argsBase64,
            gas: Number(gas),
            deposit: fnDeposit,
          }];
          break;
        }
        case "DeployContract":
          actions = [{ type: "DeployContract" as const, code: deployCode }];
          break;
        case "CreateAccount":
          actions = createDeposit && createDeposit !== "0"
            ? [{ type: "CreateAccount" as const }, { type: "Transfer" as const, deposit: createDeposit }]
            : [{ type: "CreateAccount" as const }];
          break;
        case "DeleteAccount":
          actions = [{ type: "DeleteAccount" as const, beneficiary_id: beneficiaryId }];
          break;
        case "DeleteKey":
          actions = [{ type: "DeleteKey" as const, public_key: deletePublicKey }];
          break;
        case "AddKey": {
          const permission = addKeyPermission === "FullAccess"
            ? ("FullAccess" as const)
            : {
                type: "FunctionCall" as const,
                allowance: addKeyAllowance || null,
                receiver_id: addKeyReceiverId,
                method_names: addKeyMethodNames.trim()
                  ? addKeyMethodNames.split(",").map((s) => s.trim()).filter(Boolean)
                  : [],
              };
          actions = [{ type: "AddKey" as const, public_key: addPublicKey, permission }];
          break;
        }
        case "Stake":
          actions = [{ type: "Stake" as const, stake: stakeAmount, public_key: stakePublicKey }];
          break;
      }

      const signParams = {
        signer_id: signerId,
        signer_public_key: publicKey,
        credential_source: isHardwareKey ? ("hardware_wallet" as const) : undefined,
        receiver_id: effectiveReceiverId,
        nonce: ak.nonce + 1,
        block_hash: ak.block_hash,
        actions,
        network: networkId,
        reason: receiverIsImplicit
          ? `Sign ${actionType} on ${effectiveReceiverId}`
          : `Sign ${actionType} to ${effectiveReceiverId}`,
      };
      addDebug(`sign_transaction request: ${JSON.stringify(signParams, null, 2)}`);

      let signTimeout: ReturnType<typeof setTimeout> | undefined;
      const result = await Promise.race([
        signTransaction(signParams).finally(() => clearTimeout(signTimeout)),
        new Promise<never>((_resolve, reject) => {
          signTimeout = setTimeout(() => reject(new Error(
            isHardwareKey
              ? "Ledger signing timed out. Keep the device unlocked with the NEAR app open, then try again."
              : "Signing timed out. Please try again.",
          )), SIGN_TIMEOUT_MS);
        }),
      ]);
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
      const friendly = isHardwareKey
        ? ledgerHardwareErrorMessage(msg)
        : msg;
      setError(friendly);
      if (
        isHardwareKey &&
        (msg.startsWith("ERR_HARDWARE_") || msg.startsWith("ERR_UNAVAILABLE"))
      ) {
        setHardwareError(friendly);
      }
    } finally {
      setPendingAction(null);
      setLoading(false);
    }
  }, [
    actionType,
    addDebug,
    addKeyAllowance,
    addKeyMethodNames,
    addKeyPermission,
    addKeyReceiverId,
    addPublicKey,
    argsJson,
    beneficiaryId,
    deletePublicKey,
    deployCode,
    deposit,
    formValidationMessage,
    fnDeposit,
    gas,
    methodName,
    effectiveReceiverId,
    publicKey,
    receiverId,
    selectedEvaluation,
    signerId,
    stakeAmount,
    stakePublicKey,
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

  const signerAccountId = signerId || selectedAccountId;
  const signerPublicKey =
    selectedEvaluation?.key.public_key ?? (manualMode && publicKey.trim() ? publicKey : null);

  const signerSummaryItems = useMemo(
    () => [
      {
        label: "Permission",
        value: selectedEvaluation ? (
          permissionLabel(selectedEvaluation.key)
        ) : manualMode && publicKey.trim() ? (
          "Not known"
        ) : (
          <span className="text-gray-400">Not selected</span>
        ),
      },
      {
        label: "Security",
        value: selectedEvaluation?.key.security_level ? (
          <span>{selectedEvaluation.key.security_level}</span>
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
    ],
    [
      manualMode,
      publicKey,
      selectedEvaluation,
      selectedLedgerPath,
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
        ) : publicKey.trim() ? (
          "Not known"
        ) : (
          <span className="text-gray-400">Not selected</span>
        ),
      },
      {
        label: "Security",
        value: selectedEvaluation?.key.security_level ? (
          <span>{selectedEvaluation.key.security_level}</span>
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
      {
        label: receiverIsImplicit ? "Account" : actionType === "CreateAccount" ? "New Account" : "Receiver",
        value: effectiveReceiverId ? (
          <span className="font-mono">{effectiveReceiverId}</span>
        ) : (
          <span className="text-gray-400">{receiverIsImplicit ? "Select signer account" : "Not entered"}</span>
        ),
      },
      {
        label: "Action",
        value: actionType,
      },
      ...(actionType === "Transfer"
        ? [{
            label: "Deposit",
            value: `${depositNear || "0"} NEAR • ${deposit || "0"} yocto`,
          }]
        : []),
      ...(actionType === "DeployContract"
        ? [{
            label: "Contract",
            value: deployFileName
              ? `${deployFileName} (${((deployFileSize ?? 0) / 1024).toFixed(1)} KB)`
              : <span className="text-gray-400">No file selected</span>,
          }]
        : []),
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
            {
              label: "Deposit",
              value: `${fnDepositNear || "0"} NEAR • ${fnDeposit || "0"} yocto`,
            },
          ]
        : []),
      ...(actionType === "CreateAccount"
        ? [{
            label: "Initial Deposit",
            value: `${createDepositNear || "0"} NEAR • ${createDeposit || "0"} yocto`,
          }]
        : []),
      ...(actionType === "DeleteAccount"
        ? [{
            label: "Beneficiary",
            value: beneficiaryId || <span className="text-gray-400">Not entered</span>,
          }]
        : []),
      ...(actionType === "DeleteKey"
        ? [{
            label: "Public Key",
            value: deletePublicKey ? <span className="break-all font-mono">{deletePublicKey}</span> : <span className="text-gray-400">Not entered</span>,
          }]
        : []),
      ...(actionType === "AddKey"
        ? [
            {
              label: "Public Key",
              value: addPublicKey ? <span className="break-all font-mono">{addPublicKey}</span> : <span className="text-gray-400">Not entered</span>,
            },
            {
              label: "Permission",
              value: addKeyPermission === "FullAccess" ? "Full Access" : `FunctionCall → ${addKeyReceiverId || "?"}`,
            },
          ]
        : []),
      ...(actionType === "Stake"
        ? [
            {
              label: "Stake",
              value: `${stakeAmountNear || "0"} NEAR • ${stakeAmount || "0"} yocto`,
            },
            {
              label: "Validator Key",
              value: stakePublicKey ? <span className="break-all font-mono">{stakePublicKey}</span> : <span className="text-gray-400">Not entered</span>,
            },
          ]
        : []),
    ],
    [
      actionType,
      addKeyPermission,
      addKeyReceiverId,
      addPublicKey,
      beneficiaryId,
      deletePublicKey,
      deployFileName,
      deployFileSize,
      deposit,
      depositNear,
      effectiveReceiverId,
      fnDeposit,
      fnDepositNear,
      gas,
      gasTgas,
      methodName,
      publicKey,
      receiverId,
      receiverIsImplicit,
      selectedEvaluation,
      selectedLedgerPath,
      signerId,
      stakeAmount,
      stakeAmountNear,
      stakePublicKey,
    ],
  );

  return (
    <div>
      <h1 className="mb-4 text-2xl font-bold tracking-tight text-gray-900">Sign Transaction</h1>

      <div className="mb-4 grid gap-4 lg:grid-cols-[minmax(0,1.22fr)_minmax(320px,0.78fr)]">
        <div className="min-w-0">
          <div className="rounded-lg border border-gray-200 bg-surface shadow-sm text-sm">
            <div className="border-b border-gray-100 px-4 py-3">
              <h2 className="text-xs font-medium uppercase tracking-wide text-gray-500">Transaction</h2>
            </div>

            {!receiverIsImplicit && actionType !== "CreateAccount" && (
              <div className={rowClass}>
                <span className={labelClass}>Receiver</span>
                <input
                  className={inputClass}
                  value={receiverId}
                  onChange={(e) => setReceiverId(e.target.value)}
                  placeholder="bob.near"
                />
              </div>
            )}
            {actionType === "CreateAccount" && (
              <div className={rowClass}>
                <span className={labelClass}>New Account</span>
                <div className="flex flex-1 items-center gap-0">
                  <input
                    className={inputClass + " rounded-r-none border-r-0"}
                    value={createAccountPrefix}
                    onChange={(e) => setCreateAccountPrefix(e.target.value)}
                    placeholder="sub"
                  />
                  <span className="whitespace-nowrap rounded-r-md border border-gray-300 bg-gray-100 px-2.5 py-2 text-sm text-gray-500 dark:border-gray-600 dark:bg-gray-700 dark:text-gray-400">
                    .{signerId || "account.near"}
                  </span>
                </div>
              </div>
            )}

            <div className={rowClass}>
              <span className={labelClass}>Action</span>
              <select
                className={inputClass}
                value={actionType}
                onChange={(e) => setActionType(e.target.value as ActionType)}
              >
                <option value="Transfer">Transfer</option>
                <option value="FunctionCall">Function Call</option>
                <option value="DeployContract">Deploy Contract</option>
                <option value="CreateAccount">Create Account</option>
                <option value="DeleteAccount">Delete Account</option>
                <option value="DeleteKey">Delete Key</option>
                <option value="AddKey">Add Key</option>
                <option value="Stake">Stake</option>
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

            {actionType === "DeployContract" && (
              <div
                className={`${rowClass} ${dragOver ? "bg-blue-50" : ""}`}
                onDragOver={(e) => {
                  e.preventDefault();
                  setDragOver(true);
                }}
                onDragLeave={() => setDragOver(false)}
                onDrop={handleDeployDrop}
              >
                <span className={labelClass}>WASM</span>
                <div className="flex flex-1 flex-col gap-2">
                  <button
                    type="button"
                    onClick={handlePickWasm}
                    className="rounded-md border border-gray-300 bg-gray-50 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-100"
                  >
                    Choose file…
                  </button>
                  <span className="text-xs text-gray-400">
                    or drag and drop a .wasm file here
                  </span>
                  {deployFileName && (
                    <span className="text-xs text-gray-600">
                      {deployFileName} ({((deployFileSize ?? 0) / 1024).toFixed(1)} KB)
                    </span>
                  )}
                  {deployCodeError && (
                    <span className="text-xs text-red-600">{deployCodeError}</span>
                  )}
                </div>
              </div>
            )}

            {actionType === "CreateAccount" && effectiveReceiverId && (
              <div className={rowClass}>
                <span className={labelClass}>Full ID</span>
                <span className="flex-1 text-sm font-mono text-gray-400">
                  {effectiveReceiverId}
                </span>
              </div>
            )}
            {actionType === "CreateAccount" && (
              <DualUnitInput
                label="Initial Deposit"
                primaryLabel="NEAR"
                secondaryLabel="yocto"
                primaryValue={createDepositNear}
                secondaryValue={createDeposit}
                onPrimaryChange={updateCreateDepositNear}
                onSecondaryChange={updateCreateDepositYocto}
                primaryPlaceholder="1"
                secondaryPlaceholder="1000000000000000000000000"
                error={createDepositError}
              />
            )}

            {actionType === "DeleteAccount" && (
              <div className={rowClass}>
                <span className={labelClass}>Beneficiary</span>
                <input
                  className={inputClass}
                  value={beneficiaryId}
                  onChange={(e) => setBeneficiaryId(e.target.value)}
                  placeholder="beneficiary.near"
                />
              </div>
            )}

            {actionType === "DeleteKey" && (
              <div className={rowClass}>
                <span className={labelClass}>Public Key</span>
                <input
                  className={inputClass}
                  value={deletePublicKey}
                  onChange={(e) => setDeletePublicKey(e.target.value)}
                  placeholder="ed25519:..."
                />
              </div>
            )}

            {actionType === "AddKey" && (
              <>
                <div className={rowClass}>
                  <span className={labelClass}>Public Key</span>
                  <input
                    className={inputClass}
                    value={addPublicKey}
                    onChange={(e) => setAddPublicKey(e.target.value)}
                    placeholder="ed25519:..."
                  />
                </div>
                <div className={rowClass}>
                  <span className={labelClass}>Permission</span>
                  <div className="flex flex-1 gap-0 rounded border border-gray-300 bg-gray-50 p-0.5">
                    {(["FullAccess", "FunctionCall"] as const).map((perm) => (
                      <button
                        key={perm}
                        type="button"
                        onClick={() => setAddKeyPermission(perm)}
                        className={`flex-1 rounded px-3 py-1.5 text-sm font-medium transition-colors ${
                          addKeyPermission === perm
                            ? "bg-surface text-gray-900 shadow-sm"
                            : "text-gray-500 hover:text-gray-700"
                        }`}
                      >
                        {perm === "FullAccess" ? "Full Access" : "Function Call"}
                      </button>
                    ))}
                  </div>
                </div>
                {addKeyPermission === "FunctionCall" && (
                  <>
                    <div className={rowClass}>
                      <span className={labelClass}>Receiver</span>
                      <input
                        className={inputClass}
                        value={addKeyReceiverId}
                        onChange={(e) => setAddKeyReceiverId(e.target.value)}
                        placeholder="contract.near"
                      />
                    </div>
                    <DualUnitInput
                      label="Allowance"
                      primaryLabel="NEAR"
                      secondaryLabel="yocto"
                      primaryValue={addKeyAllowanceNear}
                      secondaryValue={addKeyAllowance}
                      onPrimaryChange={updateAddKeyAllowanceNear}
                      onSecondaryChange={updateAddKeyAllowanceYocto}
                      primaryPlaceholder="0.25"
                      secondaryPlaceholder="250000000000000000000000"
                      error={addKeyAllowanceError}
                    />
                    <div className={rowClass}>
                      <span className={labelClass}>Methods</span>
                      <input
                        className={inputClass}
                        value={addKeyMethodNames}
                        onChange={(e) => setAddKeyMethodNames(e.target.value)}
                        placeholder="method1, method2 (empty = all)"
                      />
                    </div>
                  </>
                )}
              </>
            )}

            {actionType === "Stake" && (
              <>
                <DualUnitInput
                  label="Stake"
                  primaryLabel="NEAR"
                  secondaryLabel="yocto"
                  primaryValue={stakeAmountNear}
                  secondaryValue={stakeAmount}
                  onPrimaryChange={updateStakeAmountNear}
                  onSecondaryChange={updateStakeAmountYocto}
                  primaryPlaceholder="100"
                  secondaryPlaceholder="100000000000000000000000000"
                  error={stakeAmountError}
                />
                <div className={rowClass}>
                  <span className={labelClass}>Validator Key</span>
                  <input
                    className={inputClass}
                    value={stakePublicKey}
                    onChange={(e) => setStakePublicKey(e.target.value)}
                    placeholder="ed25519:..."
                  />
                </div>
              </>
            )}

            <div className="space-y-3 border-t border-gray-100 px-4 py-4">
              {error && (
                <div className="rounded-lg border border-red-200 bg-rose-50 px-4 py-3 text-sm text-rose-700">
                  {error}
                </div>
              )}

              {actionType === "FunctionCall" && argsState.error && !error && (
                <div className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">
                  {argsState.error}
                </div>
              )}

              <div className="flex flex-wrap items-center gap-3">
                {pendingAction === "sign" || pendingAction === "sign_and_broadcast" ? (
                  <div className="flex items-center gap-2 rounded-md border border-blue-200 bg-blue-50 px-4 py-2 text-sm text-blue-700">
                    <Loader2 className="size-4 animate-spin" />
                    Signing… Approve on your device if prompted.
                  </div>
                ) : (
                  <>
                    <button
                      onClick={openConfirmModal}
                      disabled={loading || Boolean(formValidationMessage)}
                      className="rounded-md bg-blue-600 px-5 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      Sign
                    </button>
                    {formValidationMessage ? (
                      <span className="text-sm text-gray-500">{formValidationMessage}</span>
                    ) : null}
                  </>
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
                    setManualMode(false);
                    void selectSignerAccount(value);
                  },
                  disabled: accountsLoading || signerAccountOptions.length === 0,
                  placeholder: "No accounts",
                  starredValues: new Set(starredAccounts),
                  onToggleStar: toggleStar,
                  meta: signerAccountId ? (
                    <CopyableValue text={signerAccountId}>
                      <Link to={`/account/${signerAccountId}`} className="text-blue-600 hover:underline">
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
                      setManualMode(false);
                      selectKey(next);
                    }
                  },
                  disabled: accountsLoading || allSignableKeyOptions.length === 0,
                  placeholder: "No keys",
                  meta: signerPublicKey ? (
                    <CopyableValue text={signerPublicKey}>
                      <span className="break-all font-mono text-gray-500">{signerPublicKey}</span>
                    </CopyableValue>
                  ) : undefined,
                }}
              />
            }
            items={signerSummaryItems}
            actions={
              <Link
                to="/settings"
                className="inline-flex items-center gap-2 rounded border border-gray-200 px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-50"
              >
                Manage keys in Settings &rarr;
              </Link>
            }
          />
        </div>
      </div>

      <SignTransactionConfirmationModal
        open={showConfirmModal}
        onClose={() => setShowConfirmModal(false)}
        onSign={() => void handleConfirmSign(false)}
        onSignAndBroadcast={() => void handleConfirmSign(true)}
        confirming={loading}
        items={confirmationItems}
        argsPreview={actionType === "FunctionCall" ? argsState.preview : null}
      />

      {broadcastSummary && (
        <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-gray-100 px-4 py-2">
            <h2
              className={`text-xs font-medium uppercase ${
                broadcastSummary.success === false ? "text-rose-600" : broadcastSummary.success === true ? "text-green-600" : "text-yellow-600"
              }`}
            >
              {broadcastSummary.success === false ? "Broadcast Failed" : broadcastSummary.success === true ? "Broadcast Result" : "Transaction Submitted"}
            </h2>
            {broadcastSummary.success !== true && (
              <button
                onClick={handleBroadcast}
                disabled={loading}
                className="rounded bg-green-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {pendingAction === "broadcast" ? "Broadcasting..." : "Broadcast again"}
              </button>
            )}
          </div>
          <div className={rowClass}>
            <span className={labelClass}>TX Hash</span>
            <Link to={`/tx/${broadcastSummary.txHash}`} className="min-w-0 flex-1 break-all text-xs text-blue-600 hover:underline font-mono">{broadcastSummary.txHash}</Link>
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

      {signResult && (
        <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-gray-100 px-4 py-2">
            <h2 className="text-xs font-medium uppercase text-gray-500">Signed Transaction</h2>
            {!broadcastSummary && (
              <button
                onClick={handleBroadcast}
                disabled={loading}
                className="rounded bg-green-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {pendingAction === "broadcast" ? "Broadcasting..." : "Broadcast"}
              </button>
            )}
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
    </div>
  );
}
