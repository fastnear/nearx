import { apiBaseUrl, nearxHeaders } from "../config";
import { fetchFastnearJson, getFastnearApiUrl } from "../tauri/runtime";

const ENDPOINT_TTL_MS: Record<string, number> = {
  blocks: 3_000,
  block: 120_000,
  account: 10_000,
  transactions: 30_000,
};
const DEFAULT_TTL_MS = 0;
const MAX_CACHE_ENTRIES = 500;
const CACHE_DEBUG = import.meta.env.DEV;

type CacheEntry = {
  value: unknown;
  expiresAt: number;
};

export type FetchApiOptions = {
  cacheTtlMs?: number;
  bypassCache?: boolean;
};

const responseCache = new Map<string, CacheEntry>();
const inflightRequests = new Map<string, Promise<unknown>>();

function debugCache(
  event: string,
  details: Record<string, unknown>,
) {
  if (!CACHE_DEBUG) {
    return;
  }

  console.debug("[api-cache]", event, details);
}

function normalizeForStableJson(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => normalizeForStableJson(item));
  }

  if (value && typeof value === "object") {
    const source = value as Record<string, unknown>;
    const normalized: Record<string, unknown> = {};
    for (const key of Object.keys(source).sort()) {
      const next = source[key];
      if (next !== undefined) {
        normalized[key] = normalizeForStableJson(next);
      }
    }
    return normalized;
  }

  return value;
}

function makeRequestKey(
  baseUrl: string,
  endpoint: string,
  body: Record<string, unknown>,
): string {
  return `${baseUrl}|${endpoint}|${JSON.stringify(normalizeForStableJson(body))}`;
}

function getTtlMs(endpoint: string): number {
  return ENDPOINT_TTL_MS[endpoint] ?? DEFAULT_TTL_MS;
}

function resolveTtlMs(endpoint: string, options?: FetchApiOptions): number {
  if (options?.bypassCache) {
    return 0;
  }

  if (options?.cacheTtlMs !== undefined) {
    return Math.max(0, options.cacheTtlMs);
  }

  return getTtlMs(endpoint);
}

function readCache<T>(key: string): { hit: true; value: T } | { hit: false } {
  const entry = responseCache.get(key);
  if (!entry) {
    return { hit: false };
  }

  if (entry.expiresAt <= Date.now()) {
    responseCache.delete(key);
    debugCache("expired", { key });
    return { hit: false };
  }

  return { hit: true, value: entry.value as T };
}

function writeCache(key: string, value: unknown, ttlMs: number) {
  if (ttlMs <= 0) {
    return;
  }

  responseCache.set(key, { value, expiresAt: Date.now() + ttlMs });

  // Keep memory usage bounded; Map preserves insertion order.
  while (responseCache.size > MAX_CACHE_ENTRIES) {
    const oldestKey = responseCache.keys().next().value as string | undefined;
    if (!oldestKey) {
      break;
    }
    responseCache.delete(oldestKey);
    debugCache("evict", { key: oldestKey, reason: "max_entries", max: MAX_CACHE_ENTRIES });
  }
}

export async function fetchApi<T>(
  endpoint: string,
  body: Record<string, unknown> = {},
  options?: FetchApiOptions,
): Promise<T> {
  const baseUrl = await getFastnearApiUrl(apiBaseUrl);
  const key = makeRequestKey(baseUrl, endpoint, body);
  const ttlMs = resolveTtlMs(endpoint, options);

  if (ttlMs > 0) {
    const cached = readCache<T>(key);
    if (cached.hit) {
      debugCache("hit", { endpoint, ttlMs, cacheSize: responseCache.size });
      return cached.value;
    }
    debugCache("miss", { endpoint, ttlMs, cacheSize: responseCache.size });
  } else {
    debugCache("cache_disabled", {
      endpoint,
      reason: options?.bypassCache ? "bypass" : "ttl_override",
    });
  }

  const inFlight = inflightRequests.get(key);
  if (inFlight) {
    debugCache("join_inflight", { endpoint, inflightCount: inflightRequests.size });
    return inFlight as Promise<T>;
  }

  const request = (async (): Promise<T> => {
    const requestUrl = `${baseUrl}/v0/${endpoint}`;
    debugCache("fetch_start", {
      endpoint,
      inflightCount: inflightRequests.size + 1,
      requestUrl,
    });
    const res = await fetchFastnearJson<T>({
      url: requestUrl,
      method: "POST",
      headers: nearxHeaders,
      body,
      include_api_key: true,
    });
    if (!res.ok) {
      debugCache("fetch_error", {
        endpoint,
        status: res.status,
        responseText: res.text,
        requestUrl: res.url,
      });
      throw new Error(`API error ${res.status}: ${res.text ?? "request failed"}`);
    }

    if (res.body == null) {
      throw new Error(`API error ${res.status}: empty JSON response`);
    }

    const data = res.body as T;
    writeCache(key, data, ttlMs);
    if (ttlMs > 0) {
      debugCache("store", { endpoint, ttlMs, cacheSize: responseCache.size });
    }
    debugCache("fetch_success", { endpoint });
    return data;
  })();

  inflightRequests.set(key, request as Promise<unknown>);

  try {
    return await request;
  } finally {
    inflightRequests.delete(key);
  }
}
