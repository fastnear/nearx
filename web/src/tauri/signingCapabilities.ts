import type {
  CredentialSource,
  ImportNearSigningKeysParams,
  SigningCapabilities,
} from "./runtime";

export const DEFAULT_LEDGER_DERIVATION_PATH = "44'/397'/0'/0'/1'";

export function secureStorageLabel(): string {
  return "secure storage";
}

export function secureStoreBackendLabel(
  capabilities: SigningCapabilities | null | undefined,
): string | null {
  switch (capabilities?.secure_store_backend) {
    case "macos_keychain":
      return "macOS Keychain";
    case "linux_secret_service":
      return "Linux Secret Service";
    case "windows_credential_manager":
      return "Windows Credential Manager";
    case "unsupported":
      return "Unavailable";
    default:
      return null;
  }
}

function localSecureStoreSourceLabel(
  capabilities: SigningCapabilities | null | undefined,
): string {
  switch (capabilities?.secure_store_backend) {
    case "macos_keychain":
      return "Keychain";
    case "linux_secret_service":
      return "Secret Service";
    case "windows_credential_manager":
      return "Credential Manager";
    default:
      return "Secure store";
  }
}

export function availableImportSources(
  capabilities: SigningCapabilities | null | undefined,
): CredentialSource[] {
  const sources: CredentialSource[] = [];
  if (capabilities?.supports_legacy_import !== false) {
    sources.push("legacy_file");
  }
  if (capabilities?.supports_near_cli_secure !== false) {
    sources.push("near_cli_secure");
  }
  return sources;
}

export function buildImportParams(
  capabilities: SigningCapabilities | null | undefined,
  params: Omit<ImportNearSigningKeysParams, "require_user_presence" | "persist_in_keychain">,
): ImportNearSigningKeysParams {
  const biometricKeychain =
    capabilities?.platform === "macos" && Boolean(capabilities?.supports_user_presence);
  return {
    ...params,
    require_user_presence: biometricKeychain,
    persist_in_keychain: Boolean(capabilities?.supports_secure_store_persistence),
    keychain_credential_protection: biometricKeychain
      ? "biometry_current_set"
      : undefined,
  };
}

export function credentialSourceLabel(
  source: CredentialSource,
  capabilities?: SigningCapabilities | null,
): string {
  switch (source) {
    case "nearxd_keychain":
      return localSecureStoreSourceLabel(capabilities);
    case "near_cli_secure":
      return "OS secrets";
    case "legacy_file":
      return "File system";
    case "hardware_wallet":
      return "Ledger";
    default:
      return source;
  }
}
