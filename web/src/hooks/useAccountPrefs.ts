import { useState, useCallback } from "react";
import type { CredentialSource } from "../tauri/runtime";

const STARRED_KEY = "nearx-starred-accounts";
const LAST_ACCOUNT_PREFIX = "nearx-last-account-";
const LAST_KEY_PREFIX = "nearx-last-key-";
const LAST_SOURCE_PREFIX = "nearx-last-source-";

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

function loadLastKey(context: string): string | null {
  return localStorage.getItem(`${LAST_KEY_PREFIX}${context}`);
}

function saveLastKey(context: string, publicKey: string) {
  localStorage.setItem(`${LAST_KEY_PREFIX}${context}`, publicKey);
}

function loadLastSource(context: string): CredentialSource | null {
  const value = localStorage.getItem(`${LAST_SOURCE_PREFIX}${context}`);
  if (!value || value.trim().startsWith("{")) {
    return null;
  }
  return value as CredentialSource | null;
}

type LastSourceMap = Record<string, CredentialSource>;

function loadLastSourceMap(context: string): LastSourceMap {
  const raw = localStorage.getItem(`${LAST_SOURCE_PREFIX}${context}`);
  if (!raw) {
    return {};
  }
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as LastSourceMap;
    }
  } catch {}
  return {};
}

function saveLastSourceMap(context: string, sourceMap: LastSourceMap) {
  localStorage.setItem(`${LAST_SOURCE_PREFIX}${context}`, JSON.stringify(sourceMap));
}

function sourceMapKey(accountId: string, publicKey: string): string {
  return `${accountId}:${publicKey}`;
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
  const [lastPublicKey, setLastPublicKey] = useState<string | null>(
    () => loadLastKey(context),
  );
  const [lastCredentialSources, setLastCredentialSources] = useState<LastSourceMap>(() =>
    loadLastSourceMap(context),
  );
  const [lastCredentialSource, setLastCredentialSource] = useState<CredentialSource | null>(
    () => loadLastSource(context),
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
      setLastAccountId((prev) => {
        if (prev === accountId) {
          return prev;
        }
        saveLastAccount(context, accountId);
        return accountId;
      });
    },
    [context],
  );

  const setLastKey = useCallback(
    (publicKey: string) => {
      setLastPublicKey((prev) => {
        if (prev === publicKey) {
          return prev;
        }
        saveLastKey(context, publicKey);
        return publicKey;
      });
    },
    [context],
  );

  const setLastSource = useCallback(
    (accountId: string, publicKey: string, source: CredentialSource | null) => {
      const key = sourceMapKey(accountId, publicKey);
      setLastCredentialSource((prev) => (prev === source ? prev : source));
      setLastCredentialSources((prev) => {
        const current = prev[key] ?? null;
        if (current === source) {
          return prev;
        }
        const next = { ...prev };
        if (source) {
          next[key] = source;
        } else {
          delete next[key];
        }
        saveLastSourceMap(context, next);
        return next;
      });
    },
    [context],
  );

  const getLastSource = useCallback(
    (accountId: string, publicKey: string) =>
      lastCredentialSources[sourceMapKey(accountId, publicKey)] ?? null,
    [lastCredentialSources],
  );

  const sortAccounts = useCallback(
    <T extends { account_id: string }>(accounts: T[]) => {
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
    lastPublicKey,
    lastCredentialSource,
    getLastSource,
    toggleStar,
    setLastAccount,
    setLastKey,
    setLastSource,
    sortAccounts,
    isStarred: useCallback(
      (accountId: string) => starredAccounts.includes(accountId),
      [starredAccounts],
    ),
  };
}
