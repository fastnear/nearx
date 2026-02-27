import { useEffect, useRef, useState } from "react";
import { getTransactions } from "../api/endpoints";
import { parseTransaction, type ParsedTx } from "../utils/parseTransaction";

const BATCH_SIZE = 20;

export default function useTxDetails(hashes: string[], key?: string) {
  const [txMap, setTxMap] = useState<Map<string, ParsedTx>>(new Map());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const knownRef = useRef<Set<string>>(new Set());
  const pendingRef = useRef(0);
  const prevKeyRef = useRef(key);

  // Reset on key change
  if (key !== prevKeyRef.current) {
    prevKeyRef.current = key;
    knownRef.current = new Set();
  }

  useEffect(() => {
    const missing = hashes.filter((h) => h && !knownRef.current.has(h));
    if (missing.length === 0) return;

    // Mark as known immediately to avoid duplicate fetches
    for (const h of missing) knownRef.current.add(h);

    // Batch into chunks of BATCH_SIZE to respect API limit
    const batches: string[][] = [];
    for (let i = 0; i < missing.length; i += BATCH_SIZE) {
      batches.push(missing.slice(i, i + BATCH_SIZE));
    }

    pendingRef.current += batches.length;
    setLoading(true);
    setError(null);

    // Fire each batch independently so earlier batches render immediately
    for (const batch of batches) {
      getTransactions(batch)
        .then((res) => {
          setTxMap((prev) => {
            const next = new Map(prev);
            for (const tx of res.transactions) {
              const parsed = parseTransaction(tx);
              next.set(parsed.hash, parsed);
            }
            return next;
          });
        })
        .catch((err) => {
          setError(String(err));
          for (const h of batch) knownRef.current.delete(h);
        })
        .finally(() => {
          pendingRef.current--;
          if (pendingRef.current === 0) setLoading(false);
        });
    }
  }, [hashes.join(",")]);

  return { txMap, loading, error };
}
