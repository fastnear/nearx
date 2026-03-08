import type {
  HardwareWalletDescriptor,
  SigningAccountEntry,
  SigningKeyEntry,
  StakingWatchlistEntry,
} from "../tauri/runtime";

export function shortPublicKey(publicKey: string): string {
  if (publicKey.length <= 30) {
    return publicKey;
  }
  return `${publicKey.slice(0, 18)}...${publicKey.slice(-8)}`;
}

export function shortAccountId(accountId: string): string {
  if (/^[0-9a-f]{64}$/i.test(accountId)) {
    return `${accountId.slice(0, 12)}...${accountId.slice(-8)}`;
  }
  return accountId;
}

export function ledgerPathTail(path: string): string {
  const trimmed = path.trim();
  if (!trimmed) {
    return "";
  }
  const parts = trimmed.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? trimmed;
}

export function ledgerPathBadge(path: string): string {
  const trimmed = path.trim();
  return trimmed || "Ledger path";
}

export function hardwareWalletSummary(
  hardwareWallet: HardwareWalletDescriptor | null | undefined,
): string | null {
  if (!hardwareWallet) {
    return null;
  }
  const tail = ledgerPathTail(hardwareWallet.derivation_path);
  if (!tail) {
    return `Ledger ${shortPublicKey(hardwareWallet.public_key)}`;
  }
  return `Ledger ${tail} · ${shortPublicKey(hardwareWallet.public_key)}`;
}

export function permissionLabel(
  key:
    | SigningKeyEntry
    | {
        permission: SigningKeyEntry["permission"];
      },
): string {
  switch (key.permission.kind) {
    case "full_access":
      return "Full Access";
    case "function_call":
      return "Function Call";
    default:
      return "Unknown";
  }
}

export function signingKeySummaryLabel(key: SigningKeyEntry): string {
  const label = key.label?.trim();
  const shortKey = shortPublicKey(key.public_key);
  return label ? `${label} · ${shortKey}` : shortKey;
}

export function signingKeyOptionLabel(key: SigningKeyEntry): string {
  const parts: string[] = [];
  const label = key.label?.trim();
  if (label) {
    parts.push(label);
  }
  const hardwareSummary = hardwareWalletSummary(key.hardware_wallet);
  if (hardwareSummary) {
    parts.push(hardwareSummary);
  } else {
    parts.push(shortPublicKey(key.public_key));
  }
  if (key.permission.kind !== "full_access") {
    parts.push(key.permission.kind === "function_call" ? "Function Call" : "Unknown");
  }
  return parts.join(" · ");
}

export function signingAccountOptionLabel(entry: SigningAccountEntry): string {
  return shortAccountId(entry.account_id);
}

export function stakingWatchlistOptionLabel(entry: StakingWatchlistEntry): string {
  const parts = [shortAccountId(entry.account_id)];
  const hardwareSummary = hardwareWalletSummary(entry.hardware_wallet);
  if (hardwareSummary) {
    parts.push(hardwareSummary);
  }
  return parts.join(" · ");
}

export function stakingWatchlistMeta(entry: StakingWatchlistEntry): string | null {
  const hardwareSummary = hardwareWalletSummary(entry.hardware_wallet);
  if (hardwareSummary) {
    return hardwareSummary;
  }
  if (entry.source === "seeded") {
    return "Imported from connected signer";
  }
  return null;
}
