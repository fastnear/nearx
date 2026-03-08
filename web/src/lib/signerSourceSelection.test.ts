import type { SigningCapabilities, SigningKeyEntry } from "../tauri/runtime";
import {
  resolveCredentialSource,
  resolveRememberedCredentialSource,
  signerSourceLabel,
} from "./signerSourceSelection";

function makeKey(overrides: Partial<SigningKeyEntry> = {}): SigningKeyEntry {
  return {
    account_id: "mike.near",
    public_key: "ed25519:test",
    curve_type: "ed25519",
    permission: { kind: "full_access" },
    available_sources: ["nearxd_keychain", "legacy_file"],
    preferred_source: "nearxd_keychain",
    in_nearxd_keychain: true,
    nearxd_keychain_protection: "unknown",
    nearxd_keychain_import_required: true,
    importable: false,
    ...overrides,
  };
}

describe("resolveCredentialSource", () => {
  it("keeps an explicitly selected blocked keychain source", () => {
    expect(resolveCredentialSource(makeKey(), "nearxd_keychain")).toBe("nearxd_keychain");
  });

  it("keeps an explicitly remembered weaker source", () => {
    expect(resolveCredentialSource(makeKey(), "legacy_file")).toBe("legacy_file");
  });

  it("keeps blocked keychain when it is the only source", () => {
    expect(
      resolveCredentialSource(
        makeKey({
          available_sources: ["nearxd_keychain"],
        }),
        "nearxd_keychain",
      ),
    ).toBe("nearxd_keychain");
  });
});

describe("resolveRememberedCredentialSource", () => {
  it("falls back from blocked keychain when reopening a remembered source", () => {
    expect(resolveRememberedCredentialSource(makeKey(), "nearxd_keychain")).toBe(
      "legacy_file",
    );
  });
});

describe("signerSourceLabel", () => {
  it("marks blocked keychain as needing fingerprint", () => {
    const capabilities = {
      secure_store_backend: "macos_keychain",
    } as SigningCapabilities;
    expect(
      signerSourceLabel("nearxd_keychain", makeKey(), capabilities),
    ).toBe(
      "Keychain (Fingerprint required)",
    );
  });
});
