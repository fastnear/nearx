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

export async function getFastnearApiUrl(defaultUrl: string): Promise<string> {
  const cfg = await getRuntimeConfigCached();
  return cfg?.fastnear_api_url ?? defaultUrl;
}

export async function getNearNodeUrl(defaultUrl: string): Promise<string> {
  const cfg = await getRuntimeConfigCached();
  return cfg?.near_node_url ?? defaultUrl;
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

export interface SignTransactionParams {
  signer_id: string;
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
}

export async function signTransaction(
  params: SignTransactionParams,
): Promise<SignTransactionResult> {
  return invoke<SignTransactionResult>("sign_transaction", { params });
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
