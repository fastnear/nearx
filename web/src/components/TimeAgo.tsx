import { useEffect, useState, useSyncExternalStore } from "react";

function formatAgo(timestampNs: string): string {
  const seconds = Math.floor((Date.now() - Number(timestampNs) / 1e6) / 1000);
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function formatFull(timestampNs: string): string {
  return new Date(Number(timestampNs) / 1e6).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function intervalFor(timestampNs: string): number {
  const seconds = Math.floor((Date.now() - Number(timestampNs) / 1e6) / 1000);
  if (seconds < 60) return 1000;
  if (seconds < 3600) return 30_000;
  return 60_000;
}

// Global toggle shared by all TimeAgo instances
let absolute = false;
const listeners = new Set<() => void>();
function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}
function getSnapshot() {
  return absolute;
}
function toggle() {
  absolute = !absolute;
  listeners.forEach((cb) => cb());
}

export default function TimeAgo({ timestampNs }: { timestampNs: string }) {
  const showAbsolute = useSyncExternalStore(subscribe, getSnapshot);
  const [, setTick] = useState(0);

  // Live-update ticks only in relative mode
  useEffect(() => {
    if (showAbsolute) return;
    const ms = intervalFor(timestampNs);
    const id = setInterval(() => setTick((n) => n + 1), ms);
    return () => clearInterval(id);
  }, [timestampNs, showAbsolute]);

  const full = formatFull(timestampNs);

  return (
    <span
      onClick={toggle}
      title={showAbsolute ? formatAgo(timestampNs) : full}
      className="cursor-pointer"
    >
      {showAbsolute ? full : formatAgo(timestampNs)}
    </span>
  );
}
