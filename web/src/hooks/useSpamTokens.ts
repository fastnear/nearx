import { useState, useEffect } from "react";

const SPAM_LIST_URL = "https://prices.intear.tech/token-spam-list";
const CACHE_KEY = "spam-token-list";
const CACHE_TTL = 60 * 60 * 1000; // 1 hour

interface CacheEntry {
  ts: number;
  ids: string[];
}

// Module-level set shared across all hook instances
let spamSet: Set<string> | null = null;

function loadFromStorage(): Set<string> | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const entry: CacheEntry = JSON.parse(raw);
    if (Date.now() - entry.ts > CACHE_TTL) return null;
    return new Set(entry.ids);
  } catch {
    return null;
  }
}

function saveToStorage(ids: string[]) {
  try {
    const entry: CacheEntry = { ts: Date.now(), ids };
    localStorage.setItem(CACHE_KEY, JSON.stringify(entry));
  } catch {
    /* quota exceeded — ignore */
  }
}

let pending: Promise<Set<string>> | null = null;

async function fetchSpamList(): Promise<Set<string>> {
  const res = await fetch(SPAM_LIST_URL);
  if (!res.ok) throw new Error(`Spam list error ${res.status}`);
  const ids: string[] = await res.json();
  saveToStorage(ids);
  return new Set(ids);
}

// Initialize from localStorage synchronously
spamSet = loadFromStorage();

export default function useSpamTokens(): Set<string> | null {
  const [spam, setSpam] = useState<Set<string> | null>(spamSet);

  useEffect(() => {
    if (spamSet) {
      setSpam(spamSet);
      return;
    }

    if (!pending) {
      pending = fetchSpamList();
    }

    pending
      .then((result) => {
        spamSet = result;
        setSpam(result);
      })
      .catch(() => {
        /* fail silently — no spam filtering */
      });
  }, []);

  return spam;
}

export function isSpam(
  spamSet: Set<string> | null,
  contractId: string
): boolean {
  return spamSet?.has(contractId) ?? false;
}
