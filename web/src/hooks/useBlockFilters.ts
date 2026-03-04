import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import type { BlockFilters } from "../api/types";

const NUMBER_KEYS: (keyof BlockFilters)[] = [
  "from_block_height",
  "to_block_height",
];

function parseFilters(params: URLSearchParams): BlockFilters {
  const filters: BlockFilters = {};
  const desc = params.get("desc");
  if (desc === "true") filters.desc = true;
  else if (desc === "false") filters.desc = false;
  for (const key of NUMBER_KEYS) {
    const val = params.get(key);
    if (val !== null) {
      const num = Number(val);
      if (!isNaN(num)) (filters as Record<string, number>)[key] = num;
    }
  }
  return filters;
}

export default function useBlockFilters() {
  const [searchParams, setSearchParams] = useSearchParams();

  const filters = useMemo(() => parseFilters(searchParams), [searchParams]);

  const setFilters = useCallback(
    (next: BlockFilters) => {
      const params = new URLSearchParams();
      if (next.desc !== undefined) params.set("desc", String(next.desc));
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
