import type { SigningKeyEntry } from "../tauri/runtime";
import { keyHasUsableSource, preferSigningKey, signingKeyId } from "./signerSourceSelection";

function makeKey(overrides: Partial<SigningKeyEntry> = {}): SigningKeyEntry {
  return {
    account_id: "mike.near",
    public_key: "ed25519:test",
    curve_type: "ed25519",
    permission: { kind: "full_access" },
    available_sources: ["nearxd_keychain", "legacy_file"],
    preferred_source: "nearxd_keychain",
    in_nearxd_keychain: true,
    importable: false,
    ...overrides,
  };
}

describe("signingKeyId", () => {
  it("combines account and public key", () => {
    expect(signingKeyId(makeKey())).toBe("mike.near:ed25519:test");
  });
});

describe("keyHasUsableSource", () => {
  it("returns true when sources are available", () => {
    expect(keyHasUsableSource(makeKey())).toBe(true);
  });

  it("returns false when no sources", () => {
    expect(keyHasUsableSource(makeKey({ available_sources: [] }))).toBe(false);
  });
});

describe("preferSigningKey", () => {
  it("prefers full-access keys with usable sources", () => {
    const fullKey = makeKey();
    const limitedKey = makeKey({
      public_key: "ed25519:limited",
      permission: { kind: "function_call", receiver_id: "x", method_names: [] },
    });
    expect(preferSigningKey([limitedKey, fullKey])).toBe(fullKey);
  });
});
