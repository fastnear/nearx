import { useState, useEffect } from "react";
import type { FtMetadata } from "./types";
import { getCachedMetadata, fetchMetadata } from "./cache";

export default function useTokenMetadata(contractId: string | null): {
  metadata: FtMetadata | null;
  loading: boolean;
} {
  const [metadata, setMetadata] = useState<FtMetadata | null>(() =>
    contractId ? getCachedMetadata(contractId) : null
  );
  const [loading, setLoading] = useState(() =>
    contractId ? !getCachedMetadata(contractId) : false
  );

  useEffect(() => {
    if (!contractId) {
      setMetadata(null);
      setLoading(false);
      return;
    }

    const cached = getCachedMetadata(contractId);
    if (cached) {
      setMetadata(cached);
      setLoading(false);
      return;
    }

    setLoading(true);
    fetchMetadata(contractId)
      .then((meta) => {
        setMetadata(meta);
        setLoading(false);
      })
      .catch(() => {
        setLoading(false);
      });
  }, [contractId]);

  return { metadata, loading };
}
