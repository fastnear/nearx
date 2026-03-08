import type {
  CredentialSource,
  SigningCapabilities,
  SigningKeyEntry,
} from "../tauri/runtime";
import { credentialSourceLabel } from "../tauri/signingCapabilities";

export function signingKeyId(
  key: Pick<SigningKeyEntry, "account_id" | "public_key">,
): string {
  return `${key.account_id}:${key.public_key}`;
}

export function resolveCredentialSource(
  key: SigningKeyEntry | null | undefined,
  selectedSource: CredentialSource | null,
): CredentialSource | null {
  if (!key) {
    return selectedSource ?? null;
  }
  const availableSources = key.available_sources;
  if (selectedSource && availableSources.includes(selectedSource)) {
    return selectedSource;
  }
  if (
    key.preferred_source &&
    availableSources.includes(key.preferred_source) &&
    !(key.nearxd_keychain_import_required && key.preferred_source === "nearxd_keychain")
  ) {
    return key.preferred_source;
  }
  if (key.nearxd_keychain_import_required) {
    return availableSources.find((source) => source !== "nearxd_keychain") ?? availableSources[0] ?? null;
  }
  return availableSources[0] ?? null;
}

export function resolveRememberedCredentialSource(
  key: SigningKeyEntry | null | undefined,
  rememberedSource: CredentialSource | null,
): CredentialSource | null {
  if (!key) {
    return rememberedSource ?? null;
  }
  const availableSources = key.available_sources;
  const fallbackSource = key.nearxd_keychain_import_required
    ? availableSources.find((source) => source !== "nearxd_keychain") ?? null
    : null;
  if (
    rememberedSource &&
    availableSources.includes(rememberedSource) &&
    !(rememberedSource === "nearxd_keychain" && fallbackSource)
  ) {
    return rememberedSource;
  }
  if (fallbackSource) {
    return fallbackSource;
  }
  return resolveCredentialSource(key, rememberedSource);
}

export function keyHasUsableSource(key: SigningKeyEntry): boolean {
  return key.available_sources.length > 0;
}

export function preferSigningKey(
  keys: SigningKeyEntry[],
  options: {
    accountId?: string | null;
    publicKey?: string | null;
  } = {},
): SigningKeyEntry | undefined {
  const { accountId, publicKey } = options;
  const withinAccount = accountId ? keys.filter((key) => key.account_id === accountId) : keys;
  const pick = (predicate: (key: SigningKeyEntry) => boolean) => withinAccount.find(predicate);
  return (
    (publicKey
      ? pick(
          (key) =>
            key.public_key === publicKey &&
            key.permission.kind === "full_access" &&
            keyHasUsableSource(key),
        )
      : undefined) ??
    pick((key) => key.permission.kind === "full_access" && keyHasUsableSource(key)) ??
    (publicKey
      ? pick((key) => key.public_key === publicKey && key.permission.kind === "full_access")
      : undefined) ??
    pick((key) => key.permission.kind === "full_access") ??
    (publicKey ? pick((key) => key.public_key === publicKey && keyHasUsableSource(key)) : undefined) ??
    pick((key) => keyHasUsableSource(key)) ??
    (publicKey ? pick((key) => key.public_key === publicKey) : undefined) ??
    withinAccount[0]
  );
}

export function fallbackLocalSource(
  key: SigningKeyEntry | null | undefined,
): CredentialSource | null {
  if (!key) {
    return null;
  }
  return key.available_sources.find((source) => source !== "hardware_wallet") ?? null;
}

export function signerSourceLabel(
  source: CredentialSource,
  key: SigningKeyEntry | null | undefined,
  signingCapabilities?: SigningCapabilities | null,
): string {
  const baseLabel = credentialSourceLabel(source, signingCapabilities);
  if (source === "nearxd_keychain" && key?.nearxd_keychain_import_required) {
    return `${baseLabel} (Fingerprint required)`;
  }
  return baseLabel;
}

export function signerSourceOptions(
  key: SigningKeyEntry | null | undefined,
  signingCapabilities?: SigningCapabilities | null,
): Array<{ value: CredentialSource; label: string }> {
  if (!key) {
    return [];
  }
  return key.available_sources.map((source) => ({
    value: source,
    label: signerSourceLabel(source, key, signingCapabilities),
  }));
}
