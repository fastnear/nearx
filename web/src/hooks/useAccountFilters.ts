import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import type { AccountFilters } from "../api/types";

const BOOLEAN_KEYS: (keyof AccountFilters)[] = [
  "is_signer",
  "is_delegated_signer",
  "is_real_signer",
  "is_any_signer",
  "is_predecessor",
  "is_explicit_refund_to",
  "is_receiver",
  "is_real_receiver",
  "is_function_call",
  "is_action_arg",
  "is_event_log",
  "is_success",
  "desc",
];

const NUMBER_KEYS: (keyof AccountFilters)[] = [
  "from_tx_block_height",
  "to_tx_block_height",
];

function parseFilters(params: URLSearchParams): AccountFilters {
  const filters: AccountFilters = {};
  for (const key of BOOLEAN_KEYS) {
    const val = params.get(key);
    if (val === "true") (filters as Record<string, boolean>)[key] = true;
    else if (val === "false") (filters as Record<string, boolean>)[key] = false;
  }
  for (const key of NUMBER_KEYS) {
    const val = params.get(key);
    if (val !== null) {
      const num = Number(val);
      if (!isNaN(num)) (filters as Record<string, number>)[key] = num;
    }
  }
  return filters;
}

export default function useAccountFilters() {
  const [searchParams, setSearchParams] = useSearchParams();

  const filters = useMemo(() => parseFilters(searchParams), [searchParams]);

  const setFilters = useCallback(
    (next: AccountFilters) => {
      const params = new URLSearchParams();
      for (const key of BOOLEAN_KEYS) {
        const val = (next as Record<string, boolean | undefined>)[key];
        if (val !== undefined) params.set(key, String(val));
      }
      for (const key of NUMBER_KEYS) {
        const val = (next as Record<string, number | undefined>)[key];
        if (val !== undefined) params.set(key, String(val));
      }
      setSearchParams(params, { replace: true });
    },
    [setSearchParams]
  );

  const filterKey = useMemo(() => JSON.stringify(filters), [filters]);

  const hasActiveFilters = useMemo(
    () => Object.keys(filters).length > 0,
    [filters]
  );

  return { filters, setFilters, filterKey, hasActiveFilters };
}
