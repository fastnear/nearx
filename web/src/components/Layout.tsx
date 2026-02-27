import { useState, useRef, useEffect, useCallback } from "react";
import { Link, Outlet } from "react-router-dom";
import SearchBar from "./SearchBar";
import { ChevronDown, Sun, Moon, Monitor } from "lucide-react";
import { networkId, otherNetworkId, otherNetworkUrl } from "../config";
import logoSvg from "../assets/logo.svg";

function NetworkSwitcher() {
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
        className="flex items-center gap-0.5 rounded bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-600 uppercase cursor-pointer hover:bg-gray-200"
      >
        {networkId}
        <ChevronDown className="size-3" />
      </button>
      {open && (
        <div className="absolute left-0 top-full mt-1 rounded border border-gray-200 bg-surface text-xs font-medium uppercase shadow-md z-10">
          <span className="block px-3 py-1.5 text-gray-900 whitespace-nowrap">
            {networkId} ✓
          </span>
          <a
            href={otherNetworkUrl}
            className="block px-3 py-1.5 text-gray-500 hover:bg-gray-50 whitespace-nowrap"
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
      className="flex items-center justify-center rounded bg-gray-100 p-1.5 text-gray-600 cursor-pointer hover:bg-gray-200"
      title={`Theme: ${themeLabel[theme]}`}
    >
      <Icon className="size-3.5" />
    </button>
  );
}

export default function Layout() {
  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <header className="border-b border-gray-200 bg-surface">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-2 sm:gap-4 px-4 py-3">
          <Link to="/" className="flex items-center gap-2 font-bold text-lg whitespace-nowrap">
            <img src={logoSvg} alt="" width="24" height="18" />
            NEAR Rocks
          </Link>
          <NetworkSwitcher />
          <ThemeToggle />
          <div className="w-full sm:w-auto sm:flex-1 order-last sm:order-none">
            <SearchBar />
          </div>
        </div>
      </header>

      <main className="mx-auto max-w-6xl px-4 py-6">
        <Outlet />
      </main>

      <footer className="border-t border-gray-200 py-4 text-center text-sm text-gray-500">
        <Link to="/" className="text-blue-600 hover:underline">NEAR Rocks</Link> &middot; <a href="https://fastnear.com" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">FastNEAR</a> &middot; <a href="https://tx.main.fastnear.com/" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">TX API</a> &middot; <a href="https://github.com/fastnear/explorer-frontend" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">GitHub</a> &middot; <a href="https://t.me/fast_near" className="text-blue-600 hover:underline" target="_blank" rel="noopener noreferrer">Feedback</a> &middot; <a href="https://x.com/fast_near" className="inline-block align-middle text-blue-600 hover:underline relative -top-px" target="_blank" rel="noopener noreferrer" title="@fast_near on X"><svg className="size-3.5" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" /></svg></a>
      </footer>
    </div>
  );
}
