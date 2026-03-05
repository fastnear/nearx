import { Link, useLocation } from "react-router-dom";
import type { ComponentType, SVGProps } from "react";
import { Blocks, Landmark } from "lucide-react";
import { isTauriRuntime } from "../tauri/runtime";

function TxIcon({ className }: SVGProps<SVGSVGElement>) {
  return (
    <span className={`inline-flex items-center justify-center font-bold text-[11px] leading-none ${className ?? ""}`}>
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
  { to: "/", icon: Blocks, label: "Explorer", match: (p) => !p.startsWith("/staking") && !p.startsWith("/sign") },
  { to: "/staking", icon: Landmark, label: "Staking", match: (p) => p.startsWith("/staking"), tauriOnly: true },
  { to: "/sign", icon: TxIcon, label: "Sign", match: (p) => p.startsWith("/sign"), tauriOnly: true },
];

export default function Sidebar() {
  const { pathname } = useLocation();
  const isTauri = isTauriRuntime();

  return (
    <nav className="hidden sm:flex flex-col shrink-0 w-12 border-r border-gray-200 bg-surface">
      {tabs.map((tab) => {
        if (tab.tauriOnly && !isTauri) return null;
        const active = tab.match(pathname);
        const Icon = tab.icon;
        return (
          <Link
            key={tab.to}
            to={tab.to}
            className={`flex flex-col items-center gap-0.5 px-1 py-2.5 text-[10px] leading-tight ${
              active
                ? "text-blue-600 bg-blue-600/10"
                : "text-gray-500 hover:text-gray-900 hover:bg-gray-50"
            }`}
            title={tab.label}
          >
            <Icon className="size-4" />
            {tab.label}
          </Link>
        );
      })}
    </nav>
  );
}
