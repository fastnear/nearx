import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

interface SignTransactionConfirmationModalProps {
  open: boolean;
  onClose: () => void;
  onSign: () => void;
  onSignAndBroadcast: () => void;
  confirming: boolean;
  items: Array<{
    label: string;
    value: ReactNode;
  }>;
  argsPreview?: string | null;
  error?: string | null;
}

export default function SignTransactionConfirmationModal({
  open,
  onClose,
  onSign,
  onSignAndBroadcast,
  confirming,
  items,
  argsPreview,
  error,
}: SignTransactionConfirmationModalProps) {
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
      className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto overscroll-contain bg-black/45 p-3 pt-4 sm:p-5 sm:pt-[10vh]"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        className="w-full max-w-4xl overflow-hidden rounded-xl border border-gray-200 bg-surface shadow-xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4 border-b border-gray-100 px-5 py-4">
          <div>
            <div className="text-lg font-semibold text-gray-900">Confirm transaction</div>
            <div className="mt-1 text-sm text-gray-500">
              Review the signer, receiver, action, and units before confirming. Your OS or Ledger
              prompt appears after you confirm.
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md border border-gray-300 p-2 text-gray-500 transition-colors hover:bg-gray-50 hover:text-gray-900 focus:ring-2 focus:ring-blue-500/20 focus:outline-none"
            title="Close transaction review"
          >
            <X className="size-4" />
          </button>
        </div>

        <div className="max-h-[70vh] overflow-y-auto px-5 py-4">
          <dl className="grid gap-x-8 gap-y-4 sm:grid-cols-2">
            {items.map((item) => (
              <div key={item.label} className="min-w-0">
                <dt className="text-xs font-medium uppercase tracking-wide text-gray-500">
                  {item.label}
                </dt>
                <dd className="mt-1 min-w-0 break-words text-base text-gray-900">{item.value}</dd>
              </div>
            ))}
          </dl>

          {argsPreview ? (
            <div className="mt-5 border-t border-gray-100 pt-4">
              <div className="mb-2 text-xs font-medium uppercase tracking-wide text-gray-500">
                Args Preview
              </div>
              <pre className="overflow-x-auto whitespace-pre-wrap rounded bg-gray-50 p-3 text-sm text-gray-700">
                {argsPreview}
              </pre>
            </div>
          ) : null}

          {error ? (
            <div className="mt-4 rounded-lg border border-rose-200 bg-rose-50 px-4 py-3 text-sm text-rose-700">
              {error}
            </div>
          ) : null}
        </div>

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-gray-100 px-5 py-4">
          <div className="text-sm text-gray-500">
            Choose whether to only sign or sign and broadcast immediately.
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={onClose}
              disabled={confirming}
              className="rounded-md border border-gray-300 px-4 py-2 text-sm text-gray-600 transition-colors hover:bg-gray-50 focus:ring-2 focus:ring-blue-500/20 focus:outline-none disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={onSign}
              disabled={confirming}
              className="rounded-md border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 transition-colors hover:bg-gray-50 focus:ring-2 focus:ring-blue-500/20 focus:outline-none disabled:opacity-50"
            >
              {confirming ? "Signing..." : "Sign"}
            </button>
            <button
              type="button"
              onClick={onSignAndBroadcast}
              disabled={confirming}
              className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:outline-none disabled:opacity-50"
            >
              {confirming ? "Signing..." : "Sign + broadcast"}
            </button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
