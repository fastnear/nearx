import { useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { Search } from "lucide-react";
import { networkId } from "../config";
import { decodeBase58 } from "../utils/format";
import {
  buildCrossNetworkAccountUrl,
  inferAccountNetwork,
} from "../utils/networkRouting";

function detectType(q: string): "block" | "tx" | "account" | null {
  if (!q) return null;
  const stripped = q.replaceAll(",", "");
  if (/^\d+$/.test(stripped)) return "block";
  if (q.length < 50 && decodeBase58(q)?.length === 32) return "tx";
  return "account";
}

const hintLabel: Record<string, string> = {
  block: "Block",
  tx: "Transaction",
  account: "Account",
};

export default function SearchBar() {
  const [query, setQuery] = useState("");
  const navigate = useNavigate();
  const type = useMemo(() => detectType(query.trim()), [query]);
  const accountTargetNetwork = useMemo(() => {
    const trimmed = query.trim();
    if (!trimmed || detectType(trimmed) !== "account") {
      return null;
    }
    return inferAccountNetwork(trimmed);
  }, [query]);
  const typeHint = useMemo(() => {
    if (!type) return null;
    if (type !== "account") return hintLabel[type];
    if (accountTargetNetwork && accountTargetNetwork !== networkId) {
      return `Account • opens ${accountTargetNetwork}`;
    }
    return hintLabel.account;
  }, [type, accountTargetNetwork]);

  function submitQuery() {
    const q = query.trim();
    const queryType = detectType(q);
    if (!q || !queryType) return;

    if (queryType === "block") {
      navigate(`/block/${q.replaceAll(",", "")}`);
    } else if (queryType === "tx") {
      navigate(`/tx/${q}`);
    } else {
      const targetNetwork = inferAccountNetwork(q);
      if (targetNetwork && targetNetwork !== networkId) {
        const targetUrl = buildCrossNetworkAccountUrl({
          currentLocation: window.location,
          targetNetwork,
          accountId: q,
        });
        window.location.assign(targetUrl);
        return;
      }
      navigate(`/account/${q}`);
    }
    setQuery("");
  }

  function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    submitQuery();
  }

  function handleInputKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key !== "Enter") return;

    const nativeEvent = e.nativeEvent as KeyboardEvent & { keyCode?: number };
    if (nativeEvent.isComposing || nativeEvent.keyCode === 229) return;

    e.preventDefault();
    submitQuery();
  }

  return (
    <form onSubmit={handleSearch} className="flex gap-2">
      <div className="relative flex-1">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleInputKeyDown}
          autoCapitalize="none"
          placeholder="Search tx, block, or account"
          className="w-full rounded-lg border border-gray-300 bg-surface px-4 py-2 pr-20 text-sm focus:border-blue-500 focus:outline-none"
        />
        {typeHint && (
          <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-gray-400">
            {typeHint}
          </span>
        )}
      </div>
      <button
        type="submit"
        className="rounded-lg bg-blue-600 px-3 py-2 sm:px-4 text-sm font-medium text-white hover:bg-blue-700"
      >
        <Search className="size-4 sm:hidden" />
        <span className="hidden sm:inline">Search</span>
      </button>
    </form>
  );
}
