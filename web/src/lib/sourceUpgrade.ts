import type {
  CredentialSource,
  SigningCapabilities,
  SigningKeyEntry,
} from "../tauri/runtime";
import { credentialSourceLabel } from "../tauri/signingCapabilities";

export type SourceUpgradeKind = "import" | "repair";

function hasImportSource(key: SigningKeyEntry): boolean {
  return key.available_sources.some(
    (source) => source === "legacy_file" || source === "near_cli_secure",
  );
}

export function resolveSourceUpgradeKind(
  key: SigningKeyEntry | null | undefined,
  effectiveSource: CredentialSource | null,
  canPersistSecureStore: boolean,
): SourceUpgradeKind | null {
  if (!key || !canPersistSecureStore || effectiveSource === "hardware_wallet") {
    return null;
  }

  if (key.nearxd_keychain_import_required) {
    return hasImportSource(key) ? "import" : "repair";
  }

  if (
    key.importable &&
    (effectiveSource === "legacy_file" || effectiveSource === "near_cli_secure")
  ) {
    return "import";
  }

  return null;
}

export function isUpgradeEligible(
  key: SigningKeyEntry | null | undefined,
  effectiveSource: CredentialSource | null,
  canPersistSecureStore: boolean,
): boolean {
  return (
    resolveSourceUpgradeKind(key, effectiveSource, canPersistSecureStore) !== null
  );
}

export function upgradeButtonLabel(
  capabilities: SigningCapabilities | null | undefined,
  kind: SourceUpgradeKind = "import",
  needsFingerprint = false,
): string {
  if (kind === "repair") {
    return "Enable fingerprint Keychain";
  }
  if (needsFingerprint) {
    return "Try fingerprint Keychain again";
  }
  return `Import to ${credentialSourceLabel("nearxd_keychain", capabilities)}`;
}

export function upgradeLoadingLabel(
  kind: SourceUpgradeKind = "import",
  needsFingerprint = false,
): string {
  if (kind === "repair") {
    return "Enabling...";
  }
  return needsFingerprint ? "Updating..." : "Importing...";
}

export function keychainUpgradeFallbackMessage(
  fallbackSource: CredentialSource | null,
  capabilities: SigningCapabilities | null | undefined,
): string {
  if (!fallbackSource) {
    return "The Keychain copy is still not fingerprint-protected. Try fingerprint Keychain again.";
  }
  return `The Keychain copy is still not fingerprint-protected. NEARx will keep using ${credentialSourceLabel(
    fallbackSource,
    capabilities,
  )} instead.`;
}
