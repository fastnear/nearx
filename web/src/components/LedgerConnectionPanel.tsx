import type { ReactNode } from "react";
import { ChevronDown, ChevronRight, Usb } from "lucide-react";
import type { HardwareWalletAccountBinding } from "../tauri/runtime";
import LedgerPathField from "./LedgerPathField";

interface LedgerConnectionPanelProps {
  open: boolean;
  onToggleOpen: () => void;
  canConnect: boolean;
  connectDisabled?: boolean;
  connecting: boolean;
  onConnect: () => void;
  connectLabel?: string;
  derivationPath: string;
  onDerivationPathChange: (value: string) => void;
  tiedToSelectedAccount: boolean;
  onTiedToSelectedAccountChange: (value: boolean) => void;
  selectedAccountLabel?: string | null;
  selectedAccountKind?: string;
  implicitAccountId?: string | null;
  publicKey?: string | null;
  accountBinding?: HardwareWalletAccountBinding | null;
  message?: string | null;
  error?: string | null;
  errorActions?: ReactNode;
}

export default function LedgerConnectionPanel({
  open,
  onToggleOpen,
  canConnect,
  connectDisabled = false,
  connecting,
  onConnect,
  connectLabel = "Connect Ledger",
  derivationPath,
  onDerivationPathChange,
  tiedToSelectedAccount,
  onTiedToSelectedAccountChange,
  selectedAccountLabel,
  selectedAccountKind = "selected account",
  implicitAccountId,
  publicKey,
  accountBinding,
  message,
  error,
  errorActions,
}: LedgerConnectionPanelProps) {
  const selectedAccount = selectedAccountLabel?.trim() ?? "";
  const needsSelectedAccount = tiedToSelectedAccount && !selectedAccount;
  const effectiveDisabled = connectDisabled || needsSelectedAccount || !canConnect;
  const modeLabel = tiedToSelectedAccount ? "Bind to selected account" : "Implicit account";

  return (
    <div className="border-t border-gray-100">
      <button
        type="button"
        onClick={onToggleOpen}
        className="flex w-full items-center justify-between px-4 py-4 text-left hover:bg-gray-50"
      >
        <span className="flex items-center gap-2">
          <span className="inline-flex size-8 items-center justify-center rounded-full bg-blue-50 text-blue-700">
            <Usb size={15} />
          </span>
          <span>
            <span className="block text-base font-medium text-gray-800">Ledger</span>
            <span className="block text-sm text-gray-500">
              Connect a device key, then use the selected account or its implicit account.
            </span>
          </span>
        </span>
        <span className="flex items-center gap-2 text-sm text-gray-500">
          <span>{open ? "Hide details" : "Show details"}</span>
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>
      </button>

      {open && (
        <div className="space-y-4 border-t border-gray-100 bg-gray-50/60 px-4 py-4">
          <div>
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-gray-500">
              Connection mode
            </div>
            <div className="inline-flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => onTiedToSelectedAccountChange(false)}
                className={`rounded-full px-3 py-1.5 text-sm font-medium ${
                  !tiedToSelectedAccount
                    ? "bg-blue-600 text-white"
                    : "border border-gray-200 text-gray-600 hover:bg-gray-50"
                }`}
              >
                Implicit account
              </button>
              <button
                type="button"
                onClick={() => onTiedToSelectedAccountChange(true)}
                className={`rounded-full px-3 py-1.5 text-sm font-medium ${
                  tiedToSelectedAccount
                    ? "bg-blue-600 text-white"
                    : "border border-gray-200 text-gray-600 hover:bg-gray-50"
                }`}
              >
                Bind to selected account
              </button>
            </div>
            <div className="mt-2 text-sm text-gray-500">{modeLabel}</div>
          </div>

          {tiedToSelectedAccount ? (
            <div
              className={`rounded border px-3 py-2.5 text-sm ${
                selectedAccount
                  ? "border-blue-100 bg-blue-50 text-blue-700"
                  : "border-amber-200 bg-amber-50 text-amber-700"
              }`}
            >
              {selectedAccount
                ? `Ledger checks that the device key is already an access key on ${selectedAccount}.`
                : `Select a ${selectedAccountKind} above before connecting this Ledger key.`}
            </div>
          ) : (
            <div className="rounded border border-slate-200 bg-slate-50 px-3 py-2.5 text-sm text-slate-700">
              NEARx will derive the standalone implicit account from the Ledger public key. The
              key does not need to be preselected above.
            </div>
          )}

          <div className="flex flex-wrap items-center gap-3">
            <LedgerPathField
              value={derivationPath}
              onChange={onDerivationPathChange}
              disabled={!canConnect || connecting}
            />
            <button
              type="button"
              onClick={onConnect}
              disabled={effectiveDisabled || connecting}
              className="inline-flex items-center gap-1 rounded bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            >
              <Usb size={13} />
              {connecting ? "Connecting..." : connectLabel}
            </button>
          </div>

          {accountBinding === "implicit_account" && implicitAccountId && (
            <div className="rounded border border-emerald-200 bg-emerald-50 px-3 py-2.5 text-sm text-emerald-800">
              Derived implicit account: <span className="font-mono">{implicitAccountId}</span>
              {publicKey ? (
                <>
                  {" "}
                  from <span className="font-mono">{publicKey}</span>
                </>
              ) : null}
            </div>
          )}

          {accountBinding === "selected_account" && selectedAccount && publicKey && (
            <div className="rounded border border-emerald-200 bg-emerald-50 px-3 py-2.5 text-sm text-emerald-800">
              Ledger key <span className="font-mono">{publicKey}</span> is bound to{" "}
              <span className="font-mono">{selectedAccount}</span>.
            </div>
          )}

          {message && (
            <div className="rounded border border-emerald-200 bg-emerald-50 px-3 py-2.5 text-sm text-emerald-800">
              {message}
            </div>
          )}

          {error && (
            <div className="rounded border border-amber-200 bg-amber-50 px-3 py-2.5 text-sm text-amber-800">
              <div>{error}</div>
              {errorActions ? <div className="mt-2">{errorActions}</div> : null}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
