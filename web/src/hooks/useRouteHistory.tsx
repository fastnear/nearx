import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";

interface RouteHistoryEntry {
  pathname: string;
  search: string;
  hash: string;
  to: string;
}

interface RouteHistoryValue {
  canGoBack: boolean;
  canGoForward: boolean;
  goBack: () => void;
  goForward: () => void;
}

interface HistoryState {
  stack: RouteHistoryEntry[];
  currentIndex: number;
}

const STORAGE_KEY = "nearx-route-history";
const MAX_ENTRIES = 16;

const RouteHistoryContext = createContext<RouteHistoryValue | null>(null);

function loadPersistedHistory(): HistoryState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const data = JSON.parse(raw);
    if (
      Array.isArray(data.stack) &&
      typeof data.currentIndex === "number" &&
      data.stack.length > 0 &&
      data.currentIndex >= 0 &&
      data.currentIndex < data.stack.length
    ) {
      return { stack: data.stack, currentIndex: data.currentIndex };
    }
  } catch {
    // ignore corrupt data
  }
  return null;
}

function persistHistory({ stack, currentIndex }: HistoryState) {
  let s = stack;
  let idx = currentIndex;
  if (s.length > MAX_ENTRIES) {
    const excess = s.length - MAX_ENTRIES;
    s = s.slice(excess);
    idx = Math.max(0, idx - excess);
  }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ stack: s, currentIndex: idx }));
  } catch {
    // storage full or unavailable
  }
}

function buildEntry(location: {
  pathname: string;
  search: string;
  hash: string;
}): RouteHistoryEntry {
  return {
    pathname: location.pathname,
    search: location.search,
    hash: location.hash,
    to: `${location.pathname}${location.search}${location.hash}`,
  };
}

/** Read the persisted last route for session restore (before provider mounts). */
export function getPersistedLastRoute(): string | null {
  const persisted = loadPersistedHistory();
  if (!persisted) return null;
  const entry = persisted.stack[persisted.currentIndex];
  return entry?.to && entry.to !== "/" ? entry.to : null;
}

export function RouteHistoryProvider({ children }: { children: ReactNode }) {
  const location = useLocation();
  const navigate = useNavigate();
  const pendingIndexRef = useRef<number | null>(null);
  const [state, setState] = useState<HistoryState>(() => {
    return loadPersistedHistory() ?? { stack: [], currentIndex: 0 };
  });

  // Persist on every state change
  useEffect(() => {
    if (state.stack.length > 0) {
      persistHistory(state);
    }
  }, [state]);

  useEffect(() => {
    const nextEntry = buildEntry(location);

    setState((prevState) => {
      const { stack, currentIndex } = prevState;

      if (stack.length === 0) {
        return {
          stack: [nextEntry],
          currentIndex: 0,
        };
      }

      const currentEntry = stack[currentIndex];
      if (currentEntry?.to === nextEntry.to) {
        return prevState;
      }

      const pendingIndex = pendingIndexRef.current;
      if (pendingIndex !== null) {
        pendingIndexRef.current = null;
        const pendingEntry = stack[pendingIndex];
        if (pendingEntry?.to === nextEntry.to) {
          if (
            pendingEntry.pathname === nextEntry.pathname &&
            pendingEntry.search === nextEntry.search &&
            pendingEntry.hash === nextEntry.hash
          ) {
            return {
              stack,
              currentIndex: pendingIndex,
            };
          }

          const nextStack = stack.slice();
          nextStack[pendingIndex] = nextEntry;
          return {
            stack: nextStack,
            currentIndex: pendingIndex,
          };
        }
      }

      if (currentEntry?.pathname === nextEntry.pathname) {
        const nextStack = stack.slice();
        nextStack[currentIndex] = nextEntry;
        return {
          stack: nextStack,
          currentIndex,
        };
      }

      const nextStack = stack.slice(0, currentIndex + 1);
      nextStack.push(nextEntry);
      return {
        stack: nextStack,
        currentIndex: nextStack.length - 1,
      };
    });
  }, [location]);

  const goBack = useCallback(() => {
    if (state.currentIndex <= 0) {
      return;
    }
    const nextIndex = state.currentIndex - 1;
    pendingIndexRef.current = nextIndex;
    navigate(state.stack[nextIndex].to);
  }, [navigate, state.currentIndex, state.stack]);

  const goForward = useCallback(() => {
    if (state.currentIndex >= state.stack.length - 1) {
      return;
    }
    const nextIndex = state.currentIndex + 1;
    pendingIndexRef.current = nextIndex;
    navigate(state.stack[nextIndex].to);
  }, [navigate, state.currentIndex, state.stack]);

  const value = useMemo<RouteHistoryValue>(
    () => ({
      canGoBack: state.currentIndex > 0,
      canGoForward: state.currentIndex < state.stack.length - 1,
      goBack,
      goForward,
    }),
    [goBack, goForward, state.currentIndex, state.stack.length],
  );

  return (
    <RouteHistoryContext.Provider value={value}>
      {children}
    </RouteHistoryContext.Provider>
  );
}

export function useRouteHistory(): RouteHistoryValue {
  const value = useContext(RouteHistoryContext);
  if (!value) {
    throw new Error("useRouteHistory must be used within a RouteHistoryProvider");
  }
  return value;
}
