import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import type { SignerModalTab } from "../lib/ledgerConnectionUi";

interface ManageSignerPanelProps {
  open: boolean;
  onClose: () => void;
  activeTab: SignerModalTab;
  onTabChange: (tab: SignerModalTab) => void;
  children: ReactNode;
}

const TABS: Array<{ id: SignerModalTab; label: string }> = [
  { id: "account_key", label: "Account & Key" },
  { id: "import", label: "Import" },
  { id: "ledger", label: "Ledger" },
];

export default function ManageSignerPanel({
  open,
  onClose,
  activeTab,
  onTabChange,
  children,
}: ManageSignerPanelProps) {
  useEffect(() => {
    if (!open) {
      return;
    }
    const previousOverflow = document.body.style.overflow;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose, open]);

  if (!open) {
    return null;
  }

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto overscroll-contain bg-black/45 p-3 pt-4 sm:p-5 sm:pt-[8vh]"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        className="flex max-h-[90vh] w-full max-w-6xl flex-col overflow-hidden rounded-xl border border-gray-200 bg-surface shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4 border-b border-gray-100 px-5 py-4">
          <div>
            <div className="text-lg font-semibold text-gray-900">Manage signer</div>
            <div className="mt-1 text-sm text-gray-500">
              Choose accounts and keys, import software keys, and connect Ledger.
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded border border-gray-200 p-2 text-gray-500 hover:bg-gray-50 hover:text-gray-900"
            title="Close signer tools"
          >
            <X className="size-4" />
          </button>
        </div>
        <div className="border-b border-gray-100 px-5 py-3">
          <div className="flex flex-wrap gap-2">
            {TABS.map((tab) => {
              const active = tab.id === activeTab;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => onTabChange(tab.id)}
                  className={`rounded-full px-3 py-1.5 text-sm font-medium ${
                    active
                      ? "bg-blue-600 text-white"
                      : "border border-gray-200 text-gray-600 hover:bg-gray-50"
                  }`}
                >
                  {tab.label}
                </button>
              );
            })}
          </div>
        </div>
        <div className="overflow-y-auto">{children}</div>
        <div className="flex items-center justify-end border-t border-gray-100 px-5 py-4">
          <button
            type="button"
            onClick={onClose}
            className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
          >
            Done
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
