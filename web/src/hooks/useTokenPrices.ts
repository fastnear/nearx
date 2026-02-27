import { useState, useEffect } from "react";

export interface TokenPrice {
  price: number;
  decimals: number;
}

const PRICES_URL = "https://1click.chaindefuser.com/v0/tokens";

// Module-level cache (shared across all hook instances)
let cache: Map<string, TokenPrice> | null = null;
let pending: Promise<Map<string, TokenPrice>> | null = null;

async function fetchPrices(): Promise<Map<string, TokenPrice>> {
  const res = await fetch(PRICES_URL);
  if (!res.ok) throw new Error(`Prices API error ${res.status}`);
  const data: Array<{
    assetId: string;
    price: number;
    decimals: number;
  }> = await res.json();

  const map = new Map<string, TokenPrice>();
  for (const entry of data) {
    if (entry.assetId.startsWith("nep141:")) {
      // e.g. "nep141:wrap.near" → key "wrap.near"
      const contractId = entry.assetId.slice(7);
      map.set(contractId, { price: entry.price, decimals: entry.decimals });
    } else if (entry.assetId.startsWith("nep245:")) {
      // e.g. "nep245:v2_1.omni.hot.tg:56_111..." → key "mt:v2_1.omni.hot.tg:56_111..."
      const rest = entry.assetId.slice(7);
      map.set("mt:" + rest, { price: entry.price, decimals: entry.decimals });
    }
  }
  return map;
}

export function computeUsdValue(
  balance: string,
  decimals: number,
  price: number
): number {
  if (!balance || balance === "0" || price === 0) return 0;
  return (Number(balance) / Math.pow(10, decimals)) * price;
}

export function getFtPrice(
  prices: Map<string, TokenPrice> | null,
  contractId: string | undefined
): TokenPrice | undefined {
  if (!prices || !contractId) return undefined;
  return prices.get(contractId);
}

export function getMtPrice(
  prices: Map<string, TokenPrice> | null,
  contractId: string | undefined,
  tokenId: string | undefined
): TokenPrice | undefined {
  if (!prices || !contractId || !tokenId) return undefined;
  return prices.get("mt:" + contractId + ":" + tokenId);
}

export function formatUsd(value: number): string {
  if (value < 0.01) return "< $0.01";
  if (value < 100_000) {
    return "$" + value.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  }
  return "$" + value.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: 0,
  });
}

export default function useTokenPrices(): {
  prices: Map<string, TokenPrice> | null;
  loading: boolean;
} {
  const [prices, setPrices] = useState<Map<string, TokenPrice> | null>(cache);
  const [loading, setLoading] = useState(cache === null);

  useEffect(() => {
    if (cache) {
      setPrices(cache);
      setLoading(false);
      return;
    }

    if (!pending) {
      pending = fetchPrices();
    }

    pending
      .then((result) => {
        cache = result;
        setPrices(result);
        setLoading(false);
      })
      .catch(() => {
        setLoading(false);
      });
  }, []);

  return { prices, loading };
}
