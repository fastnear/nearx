import { useState, useMemo, useEffect } from "react";
import JsonView from "@uiw/react-json-view";
import { darkTheme } from "@uiw/react-json-view/dark";
import { ChevronRight, ChevronDown } from "lucide-react";

type Mode = "json" | "hex" | "base64" | "raw";

function decode(base64: string): { bytes: Uint8Array; text: string } {
  const raw = atob(base64);
  const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
  return { bytes, text: raw };
}

function isPrintable(text: string): boolean {
  return /^[\x20-\x7e\t\n\r]*$/.test(text);
}

function tryParseJson(text: string): object | null {
  try {
    const parsed = JSON.parse(text);
    if (typeof parsed === "object" && parsed !== null) return parsed;
    return null;
  } catch {
    return null;
  }
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join(" ");
}

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

const btnBase =
  "px-1.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer";
const btnActive = "bg-gray-700 text-white dark:bg-gray-300 dark:text-gray-900";
const btnInactive = "bg-gray-200 text-gray-600 hover:bg-gray-300";

function useDecoded(base64: string) {
  return useMemo(() => {
    if (!base64) return { bytes: new Uint8Array(), text: "", json: null, defaultMode: "raw" as Mode, defaultExpanded: false };
    const { bytes, text } = decode(base64);
    const json = tryParseJson(text);
    let defaultMode: Mode;
    if (json) defaultMode = "json";
    else if (isPrintable(text)) defaultMode = "raw";
    else defaultMode = "hex";
    const defaultExpanded = json !== null || bytes.length < 512;
    return { bytes, text, json, defaultMode, defaultExpanded };
  }, [base64]);
}

export default function Base64Data({ base64 }: { base64: string }) {
  const { bytes, text, json, defaultMode, defaultExpanded } = useDecoded(base64);
  const isDark = useIsDark();

  const [expanded, setExpanded] = useState(defaultExpanded);
  const [mode, setMode] = useState<Mode>(defaultMode);

  if (!base64) {
    return (
      <div className="overflow-auto rounded border border-gray-200 bg-gray-50 p-2 text-xs font-mono text-gray-500">
        Empty result
      </div>
    );
  }

  const modes: { key: Mode; label: string; available: boolean }[] = [
    { key: "json", label: "JSON", available: json !== null },
    { key: "raw", label: "UTF-8", available: true },
    { key: "hex", label: "Hex", available: true },
    { key: "base64", label: "Base64", available: true },
  ];

  return (
    <div className="overflow-auto rounded border border-gray-200 bg-gray-50 p-2 text-xs">
      <div className="flex items-center gap-1.5 flex-wrap">
        <button
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-0.5 text-gray-500 hover:text-gray-800 cursor-pointer text-xs"
        >
          {expanded ? (
            <ChevronDown className="size-3.5" />
          ) : (
            <ChevronRight className="size-3.5" />
          )}
          <span>{expanded ? "Hide" : "Show"}</span>
        </button>
        <span className="text-gray-400">{bytes.length.toLocaleString()} bytes</span>
        <span className="flex gap-1 ml-auto">
          {modes
            .filter((m) => m.available)
            .map((m) => (
              <button
                key={m.key}
                onClick={() => {
                  setMode(m.key);
                  if (!expanded) setExpanded(true);
                }}
                className={`${btnBase} ${mode === m.key ? btnActive : btnInactive}`}
              >
                {m.label}
              </button>
            ))}
        </span>
      </div>

      {expanded && (
        <div className="mt-2">
          {mode === "json" && json ? (
            <JsonView
              value={json}
              collapsed={4}
              shortenTextAfterLength={512}
              displayDataTypes={false} displayObjectSize={false}
              style={isDark ? darkTheme : undefined}
            />
          ) : mode === "hex" ? (
            <div className="font-mono text-gray-700 break-all">
              {toHex(bytes)}
            </div>
          ) : mode === "base64" ? (
            <div className="font-mono text-gray-700 break-all">{base64}</div>
          ) : (
            <div className="font-mono text-gray-700 whitespace-pre-wrap break-all">
              {text}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
