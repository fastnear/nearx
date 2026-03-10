import type {
  SigningKeyEntry,
} from "../tauri/runtime";

export function signingKeyId(
  key: Pick<SigningKeyEntry, "account_id" | "public_key">,
): string {
  return `${key.account_id}:${key.public_key}`;
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
