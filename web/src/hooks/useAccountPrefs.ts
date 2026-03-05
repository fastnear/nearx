import { useState, useCallback } from "react";
import type { NearCredentialEntry } from "../tauri/runtime";

const STARRED_KEY = "nearx-starred-accounts";
const LAST_ACCOUNT_PREFIX = "nearx-last-account-";

function loadStarred(): string[] {
  try {
    const raw = localStorage.getItem(STARRED_KEY);
    if (raw) return JSON.parse(raw);
  } catch {}
  return [];
}

function saveStarred(starred: string[]) {
  localStorage.setItem(STARRED_KEY, JSON.stringify(starred));
}

function loadLastAccount(context: string): string | null {
  return localStorage.getItem(`${LAST_ACCOUNT_PREFIX}${context}`);
}

function saveLastAccount(context: string, accountId: string) {
  localStorage.setItem(`${LAST_ACCOUNT_PREFIX}${context}`, accountId);
}

// Migrate from old per-page prefs to shared format (one-time)
function migrateOldPrefs() {
  for (const oldKey of ["sign-tx-prefs", "staking-prefs"]) {
    try {
      const raw = localStorage.getItem(oldKey);
      if (!raw) continue;
      const old = JSON.parse(raw);
      // Merge starred accounts
      if (Array.isArray(old.starredAccounts) && old.starredAccounts.length > 0) {
        const existing = loadStarred();
        const merged = [...new Set([...existing, ...old.starredAccounts])];
        saveStarred(merged);
      }
      // Migrate last account
      if (old.lastAccountId) {
        const context = oldKey === "sign-tx-prefs" ? "sign" : "staking";
        if (!loadLastAccount(context)) {
          saveLastAccount(context, old.lastAccountId);
        }
      }
      localStorage.removeItem(oldKey);
    } catch {}
  }
}

migrateOldPrefs();

export function useAccountPrefs(context: string) {
  const [starredAccounts, setStarredAccounts] = useState<string[]>(loadStarred);
  const [lastAccountId, setLastAccountId] = useState<string | null>(
    () => loadLastAccount(context),
  );

  const toggleStar = useCallback((accountId: string) => {
    setStarredAccounts((prev) => {
      const set = new Set(prev);
      if (set.has(accountId)) set.delete(accountId);
      else set.add(accountId);
      const next = [...set];
      saveStarred(next);
      return next;
    });
  }, []);

  const setLastAccount = useCallback(
    (accountId: string) => {
      setLastAccountId(accountId);
      saveLastAccount(context, accountId);
    },
    [context],
  );

  const sortAccounts = useCallback(
    (accounts: NearCredentialEntry[]) => {
      const starred = new Set(starredAccounts);
      return [...accounts].sort((a, b) => {
        const aS = starred.has(a.account_id);
        const bS = starred.has(b.account_id);
        if (aS !== bS) return aS ? -1 : 1;
        return a.account_id.localeCompare(b.account_id);
      });
    },
    [starredAccounts],
  );

  return {
    starredAccounts,
    lastAccountId,
    toggleStar,
    setLastAccount,
    sortAccounts,
    isStarred: useCallback(
      (accountId: string) => starredAccounts.includes(accountId),
      [starredAccounts],
    ),
  };
}
