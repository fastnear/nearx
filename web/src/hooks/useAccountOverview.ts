import { useState, useEffect } from "react";
import {
  getAccountFull,
  type AccountFullResponse,
} from "../api/fastnearApi";

export default function useAccountOverview(accountId: string | undefined): {
  data: AccountFullResponse | null;
  loading: boolean;
  error: string | null;
} {
  const [data, setData] = useState<AccountFullResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!accountId) return;

    setData(null);
    setLoading(true);
    setError(null);

    const controller = new AbortController();

    getAccountFull(accountId, controller.signal)
      .then((result) => {
        setData(result);
        setLoading(false);
      })
      .catch((err) => {
        if (controller.signal.aborted) return;
        setError(String(err));
        setLoading(false);
      });

    return () => controller.abort();
  }, [accountId]);

  return { data, loading, error };
}
