import { Star, Download, KeyRound } from "lucide-react";
import type { NearCredentialEntry } from "../tauri/runtime";

interface AccountPickerProps {
  accounts: NearCredentialEntry[];
  selectedId: string;
  onSelect: (entry: NearCredentialEntry) => void;
  onImport: (accountId: string) => void;
  importing: boolean;
  importingAccountId?: string;
  isStarred: (id: string) => boolean;
  onToggleStar: (id: string) => void;
}

export default function AccountPicker({
  accounts,
  selectedId,
  onSelect,
  onImport,
  importing,
  importingAccountId,
  isStarred,
  onToggleStar,
}: AccountPickerProps) {
  return (
    <div className="max-h-52 overflow-y-auto">
      {accounts.map((a) => {
        const selected = a.account_id === selectedId;
        const starred = isStarred(a.account_id);
        const inKeychain = a.in_keychain !== false;
        const isImportingThis = importing && importingAccountId === a.account_id;

        return (
          <div
            key={a.account_id}
            className={`flex items-center gap-2 border-b border-gray-100 px-4 py-2 last:border-b-0 cursor-pointer ${
              selected ? "bg-blue-600/10" : "hover:bg-gray-50"
            }`}
            onClick={() => onSelect(a)}
          >
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onToggleStar(a.account_id);
              }}
              className="shrink-0 rounded p-0.5 hover:bg-gray-200"
              title={starred ? "Unstar" : "Star"}
            >
              <Star
                size={12}
                className={
                  starred
                    ? "fill-yellow-400 text-yellow-400"
                    : "text-gray-300"
                }
              />
            </button>
            <span
              className={`flex-1 min-w-0 truncate font-mono text-sm ${
                selected ? "text-gray-900 font-medium" : "text-gray-700"
              }`}
              title={a.account_id}
            >
              {a.account_id}
            </span>
            {inKeychain ? (
              <span className="shrink-0 flex items-center gap-0.5 text-xs text-green-600">
                <KeyRound size={11} />
                Keychain
              </span>
            ) : (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  onImport(a.account_id);
                }}
                disabled={importing}
                className="shrink-0 flex items-center gap-1 rounded bg-blue-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-blue-700 disabled:opacity-50"
              >
                <Download size={11} />
                {isImportingThis ? "Importing\u2026" : "Import"}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
