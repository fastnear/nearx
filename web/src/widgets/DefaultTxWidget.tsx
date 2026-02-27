import { useState, useEffect } from "react";
import type { TransactionDetail } from "../api/types";
import { ChevronRight } from "lucide-react";
import JsonView from "@uiw/react-json-view";
import { darkTheme } from "@uiw/react-json-view/dark";

function useIsDark() {
  const [dark, setDark] = useState(() => document.documentElement.classList.contains("dark"));
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setDark(document.documentElement.classList.contains("dark"));
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);
  return dark;
}

export default function DefaultTxWidget({ tx }: { tx: TransactionDetail }) {
  const [open, setOpen] = useState(true);
  const isDark = useIsDark();

  return (
    <div className="mt-4 rounded-lg border border-gray-200 bg-surface">
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full cursor-pointer items-center gap-1 px-4 py-3 text-sm font-semibold text-gray-700 hover:text-gray-900"
      >
        <ChevronRight
          className={`size-4 transition-transform ${open ? "rotate-90" : ""}`}
        />
        Raw Transaction Data
      </button>
      {open && (
        <div className="border-t border-gray-100 px-4 py-3">
          <div className="overflow-auto rounded bg-gray-50 p-3 text-xs">
            <JsonView
              value={tx}
              collapsed={2}
              shortenTextAfterLength={512}
              displayDataTypes={false} displayObjectSize={false}
              style={isDark ? darkTheme : undefined}
            />
          </div>
        </div>
      )}
    </div>
  );
}
