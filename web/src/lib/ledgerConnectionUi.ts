import type {
  ConnectHardwareWalletResult,
  CredentialSource,
  SigningKeyEntry,
} from "../tauri/runtime";

export type SignerModalTab = "account_key" | "import" | "ledger";

export interface ConnectedLedgerSnapshot {
  accountId: string;
  implicitAccountId: string;
  publicKey: string;
  label?: string | null;
  permission: ConnectHardwareWalletResult["permission"];
  availableSources: CredentialSource[];
  preferredSource: CredentialSource | null;
  derivationPath: string;
  accountBinding: ConnectHardwareWalletResult["account_binding"];
  walletType: ConnectHardwareWalletResult["wallet_type"];
  storageBackend?: string;
}

export function snapshotFromConnectResult(
  result: ConnectHardwareWalletResult,
): ConnectedLedgerSnapshot {
  return {
    accountId: result.account_id,
    implicitAccountId: result.implicit_account_id,
    publicKey: result.public_key,
    label: result.label ?? null,
    permission: result.permission,
    availableSources: result.available_sources,
    preferredSource: result.preferred_source,
    derivationPath: result.derivation_path,
    accountBinding: result.account_binding,
    walletType: result.wallet_type,
    storageBackend: result.storage_backend,
  };
}

export function snapshotMatchesKey(
  snapshot: ConnectedLedgerSnapshot | null,
  key: SigningKeyEntry | null | undefined,
): boolean {
  return Boolean(
    snapshot &&
      key &&
      snapshot.accountId === key.account_id &&
      snapshot.publicKey === key.public_key,
  );
}

export function ledgerHardwareErrorMessage(raw: string): string {
  const message = raw.trim();
  const normalized = message.toLowerCase();

  if (
    normalized.includes("err_hardware_app_not_open") ||
    normalized.includes("app not open")
  ) {
    return "Open the NEAR app on your Ledger, then try again.";
  }
  if (
    normalized.includes("err_hardware_user_rejected") ||
    normalized.includes("user rejected")
  ) {
    return "Approve the request on your Ledger to continue.";
  }
  if (
    normalized.includes("err_hardware_key_not_on_account") ||
    normalized.includes("key_not_on_account")
  ) {
    return "This Ledger key is not an access key on the selected named account. Use implicit account mode or choose a different account.";
  }
  if (
    normalized.includes("hid_read_timeout") ||
    normalized.includes("timeout") ||
    normalized.includes("err_hardware_transport")
  ) {
    return "Ledger connection timed out. Keep the device unlocked with the NEAR app open, then reconnect.";
  }
  if (normalized.includes("err_hardware_unavailable")) {
    return "Ledger is unavailable. Reconnect the device and open the NEAR app, then try again.";
  }
  return message;
}
