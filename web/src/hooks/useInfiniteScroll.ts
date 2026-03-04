import { useState, useRef, useCallback, useEffect } from "react";

interface FetchPageResult<T> {
  items: T[];
  resumeToken?: string;
  totalCount: number;
}

interface UseInfiniteScrollOptions<T> {
  fetchPage: (
    resumeToken?: string,
    limit?: number
  ) => Promise<FetchPageResult<T>>;
  batchSize: number;
  key: string;
}

interface UseInfiniteScrollReturn<T> {
  items: T[];
  totalCount: number;
  hasMore: boolean;
  loadMore: () => void;
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
  aheadItems: T[];
}

export default function useInfiniteScroll<T>({
  fetchPage,
  batchSize,
  key,
}: UseInfiniteScrollOptions<T>): UseInfiniteScrollReturn<T> {
  const [items, setItems] = useState<T[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const bufferRef = useRef<T[]>([]);
  const resumeTokenRef = useRef<string | undefined>(undefined);
  const hasMoreRef = useRef(true);
  const fetchingRef = useRef(false);
  const [, setTick] = useState(0);

  // Reset on key change
  useEffect(() => {
    setItems([]);
    setTotalCount(0);
    setLoading(true);
    setError(null);
    bufferRef.current = [];
    resumeTokenRef.current = undefined;
    hasMoreRef.current = true;
    fetchingRef.current = false;

    fetchPage(undefined, batchSize)
      .then((result) => {
        if (result.totalCount > 0) setTotalCount(result.totalCount);
        setItems(result.items);
        resumeTokenRef.current = result.resumeToken;
        if (!result.resumeToken || result.items.length < batchSize) {
          hasMoreRef.current = false;
        }
      })
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [key, fetchPage, batchSize]);

  // Background prefetch into buffer
  const prefetch = useCallback(() => {
    if (
      fetchingRef.current ||
      !hasMoreRef.current ||
      !resumeTokenRef.current ||
      bufferRef.current.length > 0
    )
      return;

    fetchingRef.current = true;
    fetchPage(resumeTokenRef.current, batchSize)
      .then((result) => {
        if (result.totalCount > 0) setTotalCount(result.totalCount);
        bufferRef.current = result.items;
        resumeTokenRef.current = result.resumeToken;
        if (!result.resumeToken || result.items.length < batchSize) {
          hasMoreRef.current = false;
        }
        setTick((n) => n + 1);
      })
      .catch(() => {
        // Silently ignore prefetch errors
      })
      .finally(() => {
        fetchingRef.current = false;
      });
  }, [fetchPage, batchSize]);

  // Trigger prefetch after items change (initial load or loadMore)
  useEffect(() => {
    if (!loading && hasMoreRef.current && bufferRef.current.length === 0) {
      prefetch();
    }
  }, [items, loading, prefetch]);

  const loadMore = useCallback(() => {
    if (!hasMoreRef.current) return;

    // If buffer has items, append instantly
    if (bufferRef.current.length > 0) {
      const buffered = bufferRef.current;
      bufferRef.current = [];
      setItems((prev) => [...prev, ...buffered]);
      return;
    }

    // If already fetching (prefetch in progress), wait for it
    if (fetchingRef.current) {
      setLoadingMore(true);
      const check = setInterval(() => {
        if (!fetchingRef.current) {
          clearInterval(check);
          if (bufferRef.current.length > 0) {
            const buffered = bufferRef.current;
            bufferRef.current = [];
            setItems((prev) => [...prev, ...buffered]);
          }
          setLoadingMore(false);
        }
      }, 50);
      return;
    }

    // Fetch directly
    if (!resumeTokenRef.current) return;
    setLoadingMore(true);
    fetchingRef.current = true;
    fetchPage(resumeTokenRef.current, batchSize)
      .then((result) => {
        if (result.totalCount > 0) setTotalCount(result.totalCount);
        setItems((prev) => [...prev, ...result.items]);
        resumeTokenRef.current = result.resumeToken;
        if (!result.resumeToken || result.items.length < batchSize) {
          hasMoreRef.current = false;
        }
      })
      .catch((err) => setError(String(err)))
      .finally(() => {
        fetchingRef.current = false;
        setLoadingMore(false);
      });
  }, [fetchPage, batchSize]);

  return {
    items,
    totalCount,
    hasMore: hasMoreRef.current,
    loadMore,
    loading,
    loadingMore,
    error,
    aheadItems: bufferRef.current,
  };
}
