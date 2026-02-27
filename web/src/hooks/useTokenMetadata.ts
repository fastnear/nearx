import { useState, useEffect } from "react";
import { networkId } from "../config";

export interface TokenMetadata {
  name: string;
  symbol: string;
  decimals: number;
  icon?: string;
}

const RPC_URL =
  networkId === "testnet"
    ? "https://rpc.testnet.fastnear.com"
    : "https://rpc.mainnet.fastnear.com";

const WELL_KNOWN: Record<string, TokenMetadata> = {
  "wrap.near": { name: "Wrapped NEAR", symbol: "wNEAR", decimals: 24 },
  "usdt.tether-token.near": { name: "Tether USD", symbol: "USDt", decimals: 6 },
  "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1": {
    name: "USDC",
    symbol: "USDC",
    decimals: 6,
  },
  "token.sweat": { name: "SWEAT", symbol: "SWEAT", decimals: 18 },
  "aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near": {
    name: "Aurora",
    symbol: "AURORA",
    decimals: 18,
  },
  "token.v2.ref-finance.near": {
    name: "Ref Finance",
    symbol: "REF",
    decimals: 18,
  },
  "intel.tkn.near": { name: "Intear", symbol: "INTEL", decimals: 18 },
  "blackdragon.tkn.near": {
    name: "BlackDragon",
    symbol: "BLACKDRAGON",
    decimals: 24,
  },
  "token.0xshitzu.near": { name: "Shitzu", symbol: "SHITZU", decimals: 18 },
  "ftv2.nekotoken.near": { name: "Neko", symbol: "NEKO", decimals: 24 },
  "game.hot.tg": { name: "HOT", symbol: "HOT", decimals: 6 },
  "aa-harvest-moon.near": { name: "Harvest Moon", symbol: "MOON", decimals: 8 },
};

// Global cache shared across all hook instances
const cache = new Map<string, TokenMetadata | null>();
const pending = new Map<string, Promise<TokenMetadata | null>>();

// Pre-populate with well-known tokens
for (const [id, meta] of Object.entries(WELL_KNOWN)) {
  cache.set(id, meta);
}

async function fetchMetadata(
  contractId: string,
): Promise<TokenMetadata | null> {
  try {
    const res = await fetch(RPC_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "ft_metadata",
        method: "query",
        params: {
          request_type: "call_function",
          finality: "final",
          account_id: contractId,
          method_name: "ft_metadata",
          args_base64: btoa("{}"),
        },
      }),
    });
    const json = await res.json();
    if (json.result?.result) {
      const decoded = new TextDecoder().decode(
        new Uint8Array(json.result.result),
      );
      const meta = JSON.parse(decoded);
      return {
        name: meta.name ?? contractId,
        symbol: meta.symbol ?? "???",
        decimals: meta.decimals ?? 0,
        icon: meta.icon,
      };
    }
  } catch {
    /* fallback to null */
  }
  return null;
}

export default function useTokenMetadata(contractId: string | null): {
  metadata: TokenMetadata | null;
  loading: boolean;
} {
  const cached = contractId ? cache.get(contractId) : undefined;
  const [metadata, setMetadata] = useState<TokenMetadata | null>(
    cached ?? null,
  );
  const [loading, setLoading] = useState(
    contractId !== null && !cache.has(contractId),
  );

  useEffect(() => {
    if (!contractId) return;

    const cached = cache.get(contractId);
    if (cached && cached.icon) {
      // Fully resolved (has icon already)
      setMetadata(cached);
      setLoading(false);
      return;
    }

    if (cached) {
      // Have metadata but missing icon — show what we have, fetch icon in background
      setMetadata(cached);
      setLoading(false);
    } else if (!cache.has(contractId)) {
      // Unknown token — full loading state
      setLoading(true);
    }

    let cancelled = false;

    // Deduplicate concurrent requests for the same contract
    let promise = pending.get(contractId);
    if (!promise) {
      promise = fetchMetadata(contractId);
      pending.set(contractId, promise);
    }

    promise.then((result) => {
      pending.delete(contractId);
      if (cached && result?.icon) {
        // Merge fetched icon into existing well-known metadata
        const updated = { ...cached, icon: result.icon };
        cache.set(contractId, updated);
      } else if (!cached) {
        cache.set(contractId, result);
      }
      if (!cancelled) {
        setMetadata(cache.get(contractId) ? { ...cache.get(contractId)! } : null);
        setLoading(false);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [contractId]);

  return { metadata, loading };
}
