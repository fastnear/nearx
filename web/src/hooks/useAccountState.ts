import { useState, useEffect } from "react";
import { viewAccount, type AccountState } from "../api/rpc";

export default function useAccountState(accountId: string | undefined): {
  data: AccountState | null;
  loading: boolean;
  error: string | null;
} {
  const [data, setData] = useState<AccountState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!accountId) return;

    setData(null);
    setLoading(true);
    setError(null);

    const controller = new AbortController();

    viewAccount(accountId, controller.signal)
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
