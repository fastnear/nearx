import { useEffect, useRef } from "react";
import { Loader2 } from "lucide-react";

interface InfiniteScrollSentinelProps {
  onLoadMore: () => void;
  hasMore: boolean;
  disabled?: boolean;
  loadingMore?: boolean;
}

export default function InfiniteScrollSentinel({
  onLoadMore,
  hasMore,
  disabled,
  loadingMore,
}: InfiniteScrollSentinelProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!hasMore || disabled) return;

    const el = ref.current;
    if (!el) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          onLoadMore();
        }
      },
      { rootMargin: "0px 0px 300px 0px" }
    );

    observer.observe(el);
    return () => observer.disconnect();
  }, [onLoadMore, hasMore, disabled]);

  if (!hasMore) return null;

  return (
    <div ref={ref} className="flex justify-center py-4">
      {loadingMore && <Loader2 className="size-5 animate-spin text-gray-400" />}
    </div>
  );
}
