import { Link, useLocation } from "react-router-dom";
import type { ComponentType, SVGProps } from "react";
import { Blocks, Landmark, Settings } from "lucide-react";
import { isTauriRuntime } from "../tauri/runtime";

function TxIcon({ className }: SVGProps<SVGSVGElement>) {
  return (
    <span className={`inline-flex items-center justify-center font-bold text-xs leading-none ${className ?? ""}`}>
      Tx
    </span>
  );
}

const tabs: Array<{
  to: string;
  icon: ComponentType<{ className?: string }>;
  label: string;
  match: (p: string) => boolean;
  tauriOnly?: boolean;
}> = [
  { to: "/", icon: Blocks, label: "Explorer", match: (p) => !p.startsWith("/staking") && !p.startsWith("/sign") && !p.startsWith("/settings") },
  { to: "/staking", icon: Landmark, label: "Staking", match: (p) => p.startsWith("/staking"), tauriOnly: true },
  { to: "/sign", icon: TxIcon, label: "Sign", match: (p) => p.startsWith("/sign"), tauriOnly: true },
  { to: "/settings", icon: Settings, label: "Settings", match: (p) => p.startsWith("/settings"), tauriOnly: true },
];

export default function Sidebar() {
  const { pathname } = useLocation();
  const isTauri = isTauriRuntime();

  return (
    <nav className="sticky top-0 hidden h-screen w-14 shrink-0 self-start flex-col border-r border-gray-200 bg-surface sm:flex">
      {tabs.map((tab) => {
        if (tab.tauriOnly && !isTauri) return null;
        const active = tab.match(pathname);
        const Icon = tab.icon;
        return (
          <Link
            key={tab.to}
            to={tab.to}
            className={`flex flex-col items-center gap-1 px-2 py-3 text-[11px] leading-tight ${
              active
                ? "text-blue-600 bg-blue-600/10"
                : "text-gray-500 hover:text-gray-900 hover:bg-gray-50"
            }`}
            title={tab.label}
          >
            <Icon className="size-5" />
            {tab.label}
          </Link>
        );
      })}
    </nav>
  );
}
