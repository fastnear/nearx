import type { SigningCapabilities, SigningKeyEntry } from "../tauri/runtime";
import {
  keychainUpgradeFallbackMessage,
  isUpgradeEligible,
  resolveSourceUpgradeKind,
  upgradeButtonLabel,
  upgradeLoadingLabel,
} from "./sourceUpgrade";

function makeKey(overrides: Partial<SigningKeyEntry> = {}): SigningKeyEntry {
  return {
    account_id: "alice.near",
    public_key: "ed25519:abc",
    curve_type: "ed25519",
    permission: { kind: "full_access" },
    available_sources: ["legacy_file", "nearxd_keychain"],
    preferred_source: null,
    in_nearxd_keychain: false,
    importable: true,
    nearxd_keychain_import_required: false,
    ...overrides,
  };
}

describe("resolveSourceUpgradeKind", () => {
  it("returns import for importable legacy-file keys", () => {
    expect(resolveSourceUpgradeKind(makeKey(), "legacy_file", true)).toBe("import");
  });

  it("returns import for near-cli secure keys", () => {
    expect(resolveSourceUpgradeKind(makeKey(), "near_cli_secure", true)).toBe("import");
  });

  it("returns import when keychain needs repair but weaker import sources still exist", () => {
    expect(
      resolveSourceUpgradeKind(
        makeKey({
          in_nearxd_keychain: true,
          nearxd_keychain_import_required: true,
          importable: false,
        }),
        "nearxd_keychain",
        true,
      ),
    ).toBe("import");
  });

  it("returns repair when keychain is the only remaining source", () => {
    expect(
      resolveSourceUpgradeKind(
        makeKey({
          available_sources: ["nearxd_keychain"],
          in_nearxd_keychain: true,
          nearxd_keychain_import_required: true,
          importable: false,
        }),
        "nearxd_keychain",
        true,
      ),
    ).toBe("repair");
  });

  it("returns null when secure store persistence is unavailable", () => {
    expect(resolveSourceUpgradeKind(makeKey(), "legacy_file", false)).toBeNull();
  });

  it("returns null for hardware wallet sources", () => {
    expect(resolveSourceUpgradeKind(makeKey(), "hardware_wallet", true)).toBeNull();
  });
});

describe("isUpgradeEligible", () => {
  it("matches the upgrade kind helper", () => {
    expect(isUpgradeEligible(makeKey(), "legacy_file", true)).toBe(true);
    expect(
      isUpgradeEligible(
        makeKey({
          available_sources: ["nearxd_keychain"],
          in_nearxd_keychain: true,
          nearxd_keychain_import_required: false,
          importable: false,
        }),
        "nearxd_keychain",
        true,
      ),
    ).toBe(false);
  });
});

describe("upgradeButtonLabel", () => {
  it("returns a repair label when repairing keychain protection", () => {
    expect(upgradeButtonLabel(null, "repair")).toBe("Enable fingerprint Keychain");
  });

  it("returns a retry label when fingerprint keychain is still blocked", () => {
    expect(upgradeButtonLabel(null, "import", true)).toBe(
      "Try fingerprint Keychain again",
    );
  });

  it("returns a macOS import label", () => {
    const caps = { secure_store_backend: "macos_keychain" } as SigningCapabilities;
    expect(upgradeButtonLabel(caps, "import")).toBe("Import to Keychain");
  });
});

describe("upgradeLoadingLabel", () => {
  it("uses distinct copy for import and repair", () => {
    expect(upgradeLoadingLabel("import")).toBe("Importing...");
    expect(upgradeLoadingLabel("repair")).toBe("Enabling...");
  });

  it("uses update copy when retrying fingerprint keychain", () => {
    expect(upgradeLoadingLabel("import", true)).toBe("Updating...");
  });
});

describe("keychainUpgradeFallbackMessage", () => {
  it("describes fallback to a weaker source", () => {
    expect(keychainUpgradeFallbackMessage("legacy_file", null)).toBe(
      "The Keychain copy is still not fingerprint-protected. NEARx will keep using File system instead.",
    );
  });

  it("uses a retry message when no weaker source is available", () => {
    expect(keychainUpgradeFallbackMessage(null, null)).toBe(
      "The Keychain copy is still not fingerprint-protected. Try fingerprint Keychain again.",
    );
  });
});
