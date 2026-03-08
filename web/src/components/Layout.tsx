import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import SearchBar from "./SearchBar";
import Sidebar from "./Sidebar";
import { ChevronDown, Sun, Moon, Monitor } from "lucide-react";
import { networkId, otherNetworkId } from "../config";
import { buildCrossNetworkUrl } from "../utils/networkRouting";
import { isTauriRuntime } from "../tauri/runtime";
import { getCurrentWindow } from "@tauri-apps/api/window";
import logoSvg from "../assets/logo.svg";
import BackForwardControls from "./BackForwardControls";
import { RouteHistoryProvider } from "../hooks/useRouteHistory";

function NetworkSwitcher({ switchUrl }: { switchUrl: string }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("click", handleClick);
    return () => document.removeEventListener("click", handleClick);
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={(e) => { e.preventDefault(); setOpen(!open); }}
        className="flex cursor-pointer items-center gap-1 rounded bg-gray-100 px-2.5 py-1 text-sm font-medium uppercase text-gray-600 hover:bg-gray-200"
      >
        {networkId}
        <ChevronDown className="size-3" />
      </button>
      {open && (
        <div className="absolute left-0 top-full z-10 mt-1 rounded border border-gray-200 bg-surface text-sm font-medium uppercase shadow-md">
          <span className="block whitespace-nowrap px-3 py-2 text-gray-900">
            {networkId} ✓
          </span>
          <a
            href={switchUrl}
            className="block whitespace-nowrap px-3 py-2 text-gray-500 hover:bg-gray-50"
          >
            {otherNetworkId}
          </a>
        </div>
      )}
    </div>
  );
}

type Theme = "light" | "dark" | "system";

function getStoredTheme(): Theme {
  const t = localStorage.getItem("theme");
  if (t === "light" || t === "dark") return t;
  return "system";
}

function applyTheme(theme: Theme) {
  const isDark =
    theme === "dark" ||
    (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", isDark);
}

const themeIcon: Record<Theme, typeof Sun> = { light: Sun, dark: Moon, system: Monitor };
const themeLabel: Record<Theme, string> = { light: "Light", dark: "Dark", system: "System" };

function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(getStoredTheme);

  useEffect(() => {
    if (theme === "system") {
      const mq = matchMedia("(prefers-color-scheme: dark)");
      const handler = () => applyTheme("system");
      mq.addEventListener("change", handler);
      return () => mq.removeEventListener("change", handler);
    }
  }, [theme]);

  const cycle = useCallback(() => {
    setTheme((prev) => {
      const systemIsDark = matchMedia("(prefers-color-scheme: dark)").matches;
      let next: Theme;
      if (prev === "system") {
        next = systemIsDark ? "light" : "dark";
      } else if (prev === "dark") {
        next = systemIsDark ? "light" : "system";
      } else {
        next = systemIsDark ? "system" : "dark";
      }
      if (next === "system") localStorage.removeItem("theme");
      else localStorage.setItem("theme", next);
      applyTheme(next);
      return next;
    });
  }, []);

  const Icon = themeIcon[theme];

  return (
    <button
      onClick={cycle}
      className="flex cursor-pointer items-center justify-center rounded bg-gray-100 p-2 text-gray-600 hover:bg-gray-200"
      title={`Theme: ${themeLabel[theme]}`}
    >
      <Icon className="size-4" />
    </button>
  );
}

function getPageTitle(pathname: string): string {
  if (pathname.startsWith("/staking")) return "NEARx — Staking";
  if (pathname.startsWith("/sign")) return "NEARx — Sign Transaction";
  if (pathname.startsWith("/tx/")) return "NEARx — Transaction";
  if (pathname.startsWith("/block/")) return "NEARx — Block";
  if (pathname.startsWith("/account/")) return "NEARx — Account";
  return "NEARx";
}

export default function Layout() {
  const location = useLocation();
  const switchUrl = useMemo(
    () =>
      buildCrossNetworkUrl({
        currentLocation: window.location,
        targetNetwork: otherNetworkId,
        routePath: location.pathname,
        search: location.search,
        hash: location.hash,
      }),
    [location.pathname, location.search, location.hash],
  );

  useEffect(() => {
    const title = getPageTitle(location.pathname);
    document.title = title;
    if (isTauriRuntime()) {
      getCurrentWindow().setTitle(title).catch(() => {});
    }
  }, [location.pathname]);

  return (
    <RouteHistoryProvider>
      <div className="flex min-h-screen flex-col bg-gray-50 text-gray-900">
        <header className="border-b border-gray-200 bg-surface">
          <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-2 px-4 py-3.5 sm:gap-4">
            <Link to="/" className="flex items-center gap-2 whitespace-nowrap text-xl font-bold">
              <img src={logoSvg} alt="" width="24" height="18" />
              NEARx
            </Link>
            <NetworkSwitcher switchUrl={switchUrl} />
            <ThemeToggle />
            <BackForwardControls />
            <div className="order-last w-full sm:order-none sm:flex-1">
              <SearchBar />
            </div>
          </div>
        </header>

        <div className="flex flex-1">
          <Sidebar />
          <div className="flex min-w-0 flex-1 flex-col">
            <main className="mx-auto w-full max-w-6xl px-4 py-6">
              <Outlet />
            </main>

            <footer className="mt-auto border-t border-gray-200 py-4 text-center text-sm text-gray-500">
              <Link to="/" className="text-blue-600 hover:underline">NEARx</Link> &middot; <a href="https://fastnear.com" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">FastNEAR</a> &middot; <a href="https://tx.main.fastnear.com/" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">TX API</a> &middot; <a href="https://github.com/fastnear/explorer-frontend" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">GitHub</a> &middot; <a href="https://t.me/fast_near" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">Feedback</a> &middot; <a href="https://x.com/fast_near" className="inline-block align-middle text-blue-600 hover:underline relative -top-px" target="_blank" rel="noopener noreferrer" title="@fast_near on X"><svg className="size-3.5" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" /></svg></a>
            </footer>
          </div>
        </div>
      </div>
    </RouteHistoryProvider>
  );
}
