import { invoke } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";

export interface RuntimeConfig {
  near_node_url: string;
  fastnear_api_url: string;
  fastnear_auth_token: string | null;
  fastnear_auth_token_source: string;
  broker_available: boolean;
}

let runtimeConfigPromise: Promise<RuntimeConfig | null> | null = null;
let signingCapabilitiesPromise: Promise<SigningCapabilities | null> | null = null;

export function isTauriRuntime(): boolean {
  return (
    import.meta.env.VITE_TAURI === "true" ||
    window.location.protocol === "tauri:"
  );
}

export async function getRuntimeConfig(): Promise<RuntimeConfig | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  try {
    return await invoke<RuntimeConfig>("get_runtime_config");
  } catch {
    return null;
  }
}

export async function getRuntimeConfigCached(): Promise<RuntimeConfig | null> {
  if (!runtimeConfigPromise) {
    runtimeConfigPromise = getRuntimeConfig();
  }
  return runtimeConfigPromise;
}

export interface SigningCapabilities {
  platform: string;
  transport: string;
  secure_store_backend: string;
  supports_legacy_import: boolean;
  supports_near_cli_secure: boolean;
  supports_secure_store_persistence: boolean;
  supports_user_presence: boolean;
  supports_hardware_wallet_connect: boolean;
  supports_hardware_wallet_sign: boolean;
}

export async function getSigningCapabilities(): Promise<SigningCapabilities | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  try {
    return await invoke<SigningCapabilities>("get_signing_capabilities");
  } catch {
    return null;
  }
}

export async function getSigningCapabilitiesCached(): Promise<SigningCapabilities | null> {
  if (!signingCapabilitiesPromise) {
    signingCapabilitiesPromise = getSigningCapabilities();
  }
  return signingCapabilitiesPromise;
}

export async function getFastnearApiUrl(defaultUrl: string): Promise<string> {
  const cfg = await getRuntimeConfigCached();
  return cfg?.fastnear_api_url ?? defaultUrl;
}

export async function getNearNodeUrl(defaultUrl: string): Promise<string> {
  const cfg = await getRuntimeConfigCached();
  return cfg?.near_node_url ?? defaultUrl;
}

export interface FastnearJsonRequestParams {
  url: string;
  method?: string;
  headers?: Record<string, string>;
  body?: unknown;
  include_api_key?: boolean;
}

export interface FastnearJsonResponse<T = unknown> {
  url: string;
  status: number;
  ok: boolean;
  body?: T | null;
  text?: string | null;
}

export async function fetchFastnearJson<T = unknown>(
  params: FastnearJsonRequestParams,
): Promise<FastnearJsonResponse<T>> {
  if (isTauriRuntime()) {
    return invoke<FastnearJsonResponse<T>>("fetch_fastnear_json", { params });
  }

  const method = params.method ?? (params.body === undefined ? "GET" : "POST");
  const response = await fetch(params.url, {
    method,
    headers: params.headers,
    body:
      params.body === undefined
        ? undefined
        : typeof params.body === "string"
          ? params.body
          : JSON.stringify(params.body),
  });
  const text = await response.text();

  let body: T | null = null;
  if (text.trim()) {
    try {
      body = JSON.parse(text) as T;
    } catch {
      body = null;
    }
  }

  return {
    url: params.url,
    status: response.status,
    ok: response.ok,
    body,
    text: !text.trim() || (response.ok && body !== null) ? null : text,
  };
}

export async function openExternal(url: string): Promise<void> {
  if (isTauriRuntime()) {
    try {
      await invoke("open_external", { url });
      return;
    } catch {
      // Fallback below
    }
  }

  window.open(url, "_blank", "noopener,noreferrer");
}

export interface UserPresenceResult {
  verified: boolean;
  platform: string;
  modality: string;
  adapter: string;
}

export async function requestUserPresence(reason?: string): Promise<UserPresenceResult> {
  return invoke<UserPresenceResult>("request_user_presence", { reason });
}

export interface NearCredentialEntry {
  account_id: string;
  public_key: string;
  in_keychain?: boolean;
}

export type CredentialSource =
  | "nearxd_keychain"
  | "near_cli_secure"
  | "legacy_file"
  | "hardware_wallet";

export type NearxdKeychainProtection =
  | "biometry_current_set"
  | "user_presence"
  | "unprotected"
  | "unknown";

export type HardwareWalletType = "ledger";

export type AccessPermission =
  | { kind: "full_access" }
  | {
      kind: "function_call";
      receiver_id: string;
      method_names: string[];
      allowance?: string | null;
    }
  | { kind: "unknown" };

export interface HardwareWalletDescriptor {
  wallet_type: HardwareWalletType;
  public_key: string;
  derivation_path: string;
}

export interface SigningKeyEntry {
  account_id: string;
  public_key: string;
  label?: string | null;
  curve_type: "ed25519" | "secp256k1" | "unknown";
  permission: AccessPermission;
  available_sources: CredentialSource[];
  preferred_source: CredentialSource | null;
  in_nearxd_keychain: boolean;
  nearxd_keychain_protection?: NearxdKeychainProtection | null;
  nearxd_keychain_import_required?: boolean;
  importable: boolean;
  last_seen_at_ms?: number | null;
  stale?: boolean;
  hardware_wallet?: HardwareWalletDescriptor | null;
}

export interface SigningAccountEntry {
  account_id: string;
  has_keys: boolean;
  source_hints: CredentialSource[];
  hardware_wallets?: HardwareWalletDescriptor[];
}

export interface ListNearSigningAccountsResult {
  network: string;
  credentials_home_dir: string;
  accounts: SigningAccountEntry[];
}

export interface ListNearSigningKeysResult {
  network: string;
  credentials_home_dir: string;
  keys: SigningKeyEntry[];
}

export interface ListNearSigningParams {
  network: string;
  account_id?: string;
  credentials_home_dir?: string;
}

export async function listNearSigningAccounts(
  params: ListNearSigningParams,
): Promise<ListNearSigningAccountsResult> {
  return invoke<ListNearSigningAccountsResult>("list_near_signing_accounts", {
    params,
  });
}

export async function listNearSigningKeys(
  params: ListNearSigningParams,
): Promise<ListNearSigningKeysResult> {
  return invoke<ListNearSigningKeysResult>("list_near_signing_keys", {
    params,
  });
}

export interface ListNearCredentialsResult {
  network: string;
  credentials_dir: string;
  accounts: NearCredentialEntry[];
}

export async function listNearCredentials(
  network: string,
): Promise<ListNearCredentialsResult> {
  return invoke<ListNearCredentialsResult>("list_near_credentials", {
    network,
  });
}

export interface ImportNearCredentialsParams {
  network: string;
  account_id?: string;
  require_user_presence?: boolean;
  persist_in_keychain?: boolean;
}

export interface ImportedCredential {
  account_id: string;
  public_key: string;
  keychain_status?: string;
  storage_backend?: string;
}

export interface ImportNearCredentialsResult {
  network: string;
  imported_count: number;
  imported: ImportedCredential[];
  skipped: ImportedCredential[];
  failed: Array<{ account_id: string; error: string }>;
}

export async function importNearCredentials(
  params: ImportNearCredentialsParams,
): Promise<ImportNearCredentialsResult> {
  return invoke<ImportNearCredentialsResult>("import_near_credentials", {
    params,
  });
}

export interface ImportNearSigningKeysParams {
  network: string;
  account_id?: string;
  public_key?: string;
  source?: CredentialSource;
  sources?: CredentialSource[];
  require_user_presence?: boolean;
  persist_in_keychain?: boolean;
  keychain_credential_protection?: "biometry_current_set" | "user_presence";
  allow_fallback?: boolean;
  overwrite?: boolean;
  save_settings?: boolean;
  max_keys?: number;
}

export interface ImportedSigningKey {
  account_id: string;
  public_key: string;
  label?: string | null;
  curve_type: "ed25519" | "secp256k1" | "unknown";
  source: CredentialSource;
  keychain_status?: string;
  keychain_protection?: NearxdKeychainProtection | null;
  keychain_account?: string;
  storage_backend?: string;
}

export interface ImportNearSigningKeysResult {
  network: string;
  credentials_home_dir: string;
  credentials_dir: string;
  imported_count: number;
  imported: ImportedSigningKey[];
  skipped: ImportedSigningKey[];
  storage_backend?: string;
  failed: Array<{
    account_id?: string;
    public_key?: string;
    source?: CredentialSource;
    error: string;
  }>;
}

export async function importNearSigningKeys(
  params: ImportNearSigningKeysParams,
): Promise<ImportNearSigningKeysResult> {
  return invoke<ImportNearSigningKeysResult>("import_near_signing_keys", {
    params,
  });
}

export interface ReprotectNearSigningKeyParams {
  network: string;
  account_id: string;
  public_key: string;
  reason?: string;
}

export interface ReprotectNearSigningKeyResult {
  network: string;
  account_id: string;
  public_key: string;
  keychain_status?: string;
  keychain_protection?: NearxdKeychainProtection | null;
  storage_backend?: string;
}

export async function reprotectNearSigningKey(
  params: ReprotectNearSigningKeyParams,
): Promise<ReprotectNearSigningKeyResult> {
  return invoke<ReprotectNearSigningKeyResult>("reprotect_near_signing_key", {
    params,
  });
}

export interface SetSigningKeyLabelParams {
  network: string;
  account_id: string;
  public_key: string;
  label: string | null;
}

export interface SetSigningKeyLabelResult {
  network: string;
  account_id: string;
  public_key: string;
  label?: string | null;
}

export async function setSigningKeyLabel(
  params: SetSigningKeyLabelParams,
): Promise<SetSigningKeyLabelResult> {
  return invoke<SetSigningKeyLabelResult>("set_signing_key_label", { params });
}

export interface SignTransactionParams {
  signer_id: string;
  signer_public_key?: string;
  credential_source?: CredentialSource;
  receiver_id: string;
  nonce: number;
  block_hash: string;
  actions: Array<{
    type: string;
    deposit?: string;
    method_name?: string;
    args?: string;
    gas?: number;
  }>;
  network?: string;
  reason?: string;
}

export interface SignTransactionResult {
  signed_transaction_base64: string;
  tx_hash: string;
  signer_id: string;
  public_key: string;
  credential_source?: CredentialSource;
}

export async function signTransaction(
  params: SignTransactionParams,
): Promise<SignTransactionResult> {
  return invoke<SignTransactionResult>("sign_transaction", { params });
}

export interface StakingWatchlistEntry {
  network: string;
  account_id: string;
  added_at_ms: number;
  source: "manual" | "seeded" | "hardware_wallet";
  hardware_wallet?: HardwareWalletDescriptor | null;
}

export interface ListStakingWatchlistResult {
  network: string;
  entries: StakingWatchlistEntry[];
}

export interface UpsertStakingWatchlistResult {
  network: string;
  account_id: string;
  removed?: boolean;
  entries: StakingWatchlistEntry[];
}

export interface StakingWatchlistParams {
  network: string;
}

export interface StakingWatchlistAccountParams extends StakingWatchlistParams {
  account_id: string;
  source?: "manual" | "seeded" | "hardware_wallet";
  wallet_type?: HardwareWalletType;
  public_key?: string;
  derivation_path?: string;
}

export async function listStakingWatchlist(
  params: StakingWatchlistParams,
): Promise<ListStakingWatchlistResult> {
  return invoke<ListStakingWatchlistResult>("list_staking_watchlist", { params });
}

export async function addStakingWatchlistAccount(
  params: StakingWatchlistAccountParams,
): Promise<UpsertStakingWatchlistResult> {
  return invoke<UpsertStakingWatchlistResult>("add_staking_watchlist_account", { params });
}

export async function removeStakingWatchlistAccount(
  params: StakingWatchlistAccountParams,
): Promise<UpsertStakingWatchlistResult> {
  return invoke<UpsertStakingWatchlistResult>("remove_staking_watchlist_account", { params });
}

export interface ConnectHardwareWalletParams {
  network: string;
  wallet_type: HardwareWalletType;
  account_id?: string;
  derivation_path?: string;
}

export type HardwareWalletAccountBinding =
  | "selected_account"
  | "implicit_account";

export interface ConnectHardwareWalletResult {
  network: string;
  wallet_type: HardwareWalletType;
  account_id: string;
  requested_account_id?: string | null;
  implicit_account_id: string;
  account_binding: HardwareWalletAccountBinding;
  public_key: string;
  label?: string | null;
  curve_type: "ed25519" | "secp256k1" | "unknown";
  permission: AccessPermission;
  available_sources: CredentialSource[];
  preferred_source: CredentialSource | null;
  in_nearxd_keychain: boolean;
  importable: boolean;
  derivation_path: string;
  storage_backend?: string;
}

export async function connectHardwareWallet(
  params: ConnectHardwareWalletParams,
): Promise<ConnectHardwareWalletResult> {
  return invoke<ConnectHardwareWalletResult>("connect_hardware_wallet", { params });
}

export async function subscribeDeepLinks(
  onUrl: (url: string) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }

  try {
    const current = (await getCurrent()) ?? [];
    for (const url of current) {
      onUrl(url);
    }

    return await onOpenUrl((urls) => {
      for (const url of urls) {
        onUrl(url);
      }
    });
  } catch {
    return () => {};
  }
}
