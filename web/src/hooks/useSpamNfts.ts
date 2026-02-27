import { useState, useEffect } from "react";

const SPAM_LIST_URL = "https://nft-proxy-service.intear.tech/spam-list";
const CACHE_KEY = "spam-nft-list";
const CACHE_TTL = 60 * 60 * 1000; // 1 hour

interface SpamEntry {
  Collection?: string;
  Token?: [string, string];
}

export interface SpamNftData {
  collections: Set<string>;
  tokens: Set<string>; // "contract:tokenId"
}

interface CacheEntry {
  ts: number;
  collections: string[];
  tokens: string[];
}

// Module-level data shared across all hook instances
let spamData: SpamNftData | null = null;

function loadFromStorage(): SpamNftData | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const entry: CacheEntry = JSON.parse(raw);
    if (Date.now() - entry.ts > CACHE_TTL) return null;
    return {
      collections: new Set(entry.collections),
      tokens: new Set(entry.tokens),
    };
  } catch {
    return null;
  }
}

function saveToStorage(data: SpamNftData) {
  try {
    const entry: CacheEntry = {
      ts: Date.now(),
      collections: [...data.collections],
      tokens: [...data.tokens],
    };
    localStorage.setItem(CACHE_KEY, JSON.stringify(entry));
  } catch {
    /* quota exceeded — ignore */
  }
}

let pending: Promise<SpamNftData> | null = null;

async function fetchSpamList(): Promise<SpamNftData> {
  const res = await fetch(SPAM_LIST_URL);
  if (!res.ok) throw new Error(`NFT spam list error ${res.status}`);
  const entries: SpamEntry[] = await res.json();
  const collections = new Set<string>();
  const tokens = new Set<string>();
  for (const e of entries) {
    if (e.Collection) collections.add(e.Collection);
    if (e.Token) tokens.add(`${e.Token[0]}:${e.Token[1]}`);
  }
  const data: SpamNftData = { collections, tokens };
  saveToStorage(data);
  return data;
}

// Initialize from localStorage synchronously
spamData = loadFromStorage();

export default function useSpamNfts(): SpamNftData | null {
  const [spam, setSpam] = useState<SpamNftData | null>(spamData);

  useEffect(() => {
    if (spamData) {
      setSpam(spamData);
      return;
    }

    if (!pending) {
      pending = fetchSpamList();
    }

    pending
      .then((result) => {
        spamData = result;
        setSpam(result);
      })
      .catch(() => {
        /* fail silently — no spam filtering */
      });
  }, []);

  return spam;
}

export function isSpamNft(
  data: SpamNftData | null,
  contractId: string,
  tokenId?: string
): boolean {
  if (!data) return false;
  if (data.collections.has(contractId)) return true;
  if (tokenId && data.tokens.has(`${contractId}:${tokenId}`)) return true;
  return false;
}
