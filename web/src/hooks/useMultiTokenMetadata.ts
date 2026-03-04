import { useState, useEffect } from "react";
import { networkId } from "../config";

export interface MultiTokenMetadata {
  title: string;
  symbol?: string;
  decimals?: number;
  media?: string;
  description?: string;
}

const RPC_URL =
  networkId === "testnet"
    ? "https://rpc.testnet.fastnear.com"
    : "https://rpc.mainnet.fastnear.com";

// Global cache keyed by "contractId:tokenId"
const cache = new Map<string, MultiTokenMetadata | null>();
const pending = new Map<string, Promise<MultiTokenMetadata | null>>();

function cacheKey(contractId: string, tokenId: string): string {
  return `${contractId}:${tokenId}`;
}

async function fetchMtMetadata(
  contractId: string,
  tokenId: string,
): Promise<MultiTokenMetadata | null> {
  try {
    const res = await fetch(RPC_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "mt_metadata",
        method: "query",
        params: {
          request_type: "call_function",
          finality: "final",
          account_id: contractId,
          method_name: "mt_metadata_base_by_token_id",
          args_base64: btoa(JSON.stringify({ token_ids: [tokenId] })),
        },
      }),
    });
    const json = await res.json();
    if (json.result?.result) {
      const decoded = new TextDecoder().decode(
        new Uint8Array(json.result.result),
      );
      const arr = JSON.parse(decoded);
      if (Array.isArray(arr) && arr.length > 0 && arr[0]) {
        const meta = arr[0];
        return {
          title: meta.name ?? meta.title ?? tokenId,
          symbol: meta.symbol,
          decimals: typeof meta.decimals === "number" ? meta.decimals : undefined,
          media: meta.icon ?? meta.media,
          description: meta.description,
        };
      }
    }
  } catch {
    /* fallback to null */
  }
  return null;
}

export default function useMultiTokenMetadata(
  contractId: string | null,
  tokenId: string | null,
): {
  metadata: MultiTokenMetadata | null;
  loading: boolean;
} {
  const key =
    contractId && tokenId ? cacheKey(contractId, tokenId) : null;
  const cached = key ? cache.get(key) : undefined;
  const [metadata, setMetadata] = useState<MultiTokenMetadata | null>(
    cached ?? null,
  );
  const [loading, setLoading] = useState(
    key !== null && !cache.has(key),
  );

  useEffect(() => {
    if (!key || !contractId || !tokenId) return;

    if (cache.has(key)) {
      setMetadata(cache.get(key) ?? null);
      setLoading(false);
      return;
    }

    setLoading(true);
    let cancelled = false;

    let promise = pending.get(key);
    if (!promise) {
      promise = fetchMtMetadata(contractId, tokenId);
      pending.set(key, promise);
    }

    promise.then((result) => {
      cache.set(key, result);
      pending.delete(key);
      if (!cancelled) {
        setMetadata(result);
        setLoading(false);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [key, contractId, tokenId]);

  return { metadata, loading };
}
