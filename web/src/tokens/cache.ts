import type { FtMetadata } from "./types";
import { getWellKnownToken } from "./well-known";
import { viewCall } from "../api/rpc";
import { networkId } from "../config";

const STORAGE_KEY = "ft-metadata-cache";
const TTL_MS = 7 * 24 * 60 * 60 * 1000; // 7 days

const memoryCache = new Map<string, FtMetadata>();
const inflight = new Map<string, Promise<FtMetadata>>();

function loadStorageCache(): Record<string, FtMetadata> {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || "{}");
  } catch {
    return {};
  }
}

function saveToStorage(meta: FtMetadata) {
  try {
    const cache = loadStorageCache();
    cache[meta.contractId] = meta;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(cache));
  } catch {
    // localStorage full or unavailable
  }
}

export function getCachedMetadata(contractId: string): FtMetadata | null {
  // 1. Memory cache
  const mem = memoryCache.get(contractId);
  if (mem) return mem;

  // 2. Well-known tokens
  const wellKnown = getWellKnownToken(contractId, networkId);
  if (wellKnown) {
    memoryCache.set(contractId, wellKnown);
    return wellKnown;
  }

  // 3. localStorage
  const stored = loadStorageCache()[contractId];
  if (stored && (stored.fetchedAt === 0 || Date.now() - stored.fetchedAt < TTL_MS)) {
    memoryCache.set(contractId, stored);
    return stored;
  }

  return null;
}

export async function fetchMetadata(contractId: string): Promise<FtMetadata> {
  // Check cache first
  const cached = getCachedMetadata(contractId);
  if (cached) return cached;

  // Deduplicate concurrent fetches
  const existing = inflight.get(contractId);
  if (existing) return existing;

  const promise = (async () => {
    try {
      const raw = await viewCall<{ name: string; symbol: string; decimals: number; icon?: string }>(
        contractId,
        "ft_metadata"
      );
      const meta: FtMetadata = {
        name: raw.name,
        symbol: raw.symbol,
        decimals: raw.decimals,
        icon: raw.icon,
        contractId,
        fetchedAt: Date.now(),
      };
      memoryCache.set(contractId, meta);
      saveToStorage(meta);
      return meta;
    } finally {
      inflight.delete(contractId);
    }
  })();

  inflight.set(contractId, promise);
  return promise;
}
