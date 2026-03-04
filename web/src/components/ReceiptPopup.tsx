import { useEffect, useMemo, useRef } from "react";
import { createPortal } from "react-dom";
import type { ReceiptWithOutcome } from "../api/types";
import { parseAction } from "../utils/parseTransaction";
import type { ParsedAction } from "../utils/parseTransaction";
import ReceiptCard from "./ReceiptCard";
import { X } from "lucide-react";

function findReceiptForAction(
  receipts: ReceiptWithOutcome[],
  action: ParsedAction,
  actionIndex: number
): ReceiptWithOutcome | null {
  // For each receipt, extract its actions and check for a match
  for (const r of receipts) {
    const actionData = (r.receipt.receipt as Record<string, unknown>)
      ?.Action as Record<string, unknown> | undefined;
    if (!actionData) continue;
    const receiptActions = (actionData.actions as unknown[]) ?? [];
    for (let i = 0; i < receiptActions.length; i++) {
      const parsed = parseAction(receiptActions[i] as Record<string, unknown>);
      if (parsed.type === action.type && parsed.method_name === action.method_name) {
        if (i === actionIndex || receiptActions.length === 1) {
          return r;
        }
      }
    }
  }
  // Fallback: return first non-system Action receipt
  for (const r of receipts) {
    if (r.receipt.predecessor_id === "system") continue;
    const actionData = (r.receipt.receipt as Record<string, unknown>)?.Action;
    if (actionData) return r;
  }
  return receipts[0] ?? null;
}

export default function ReceiptPopup({
  txHash,
  receipts,
  action,
  actionIndex,
  onClose,
}: {
  txHash: string;
  receipts: ReceiptWithOutcome[];
  action: ParsedAction;
  actionIndex: number;
  onClose: () => void;
}) {
  const overlayRef = useRef<HTMLDivElement>(null);

  const receipt = useMemo(
    () => findReceiptForAction(receipts, action, actionIndex),
    [receipts, action, actionIndex]
  );

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKey);
    // Lock body scroll while popup is open
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", handleKey);
      document.body.style.overflow = prev;
    };
  }, [onClose]);

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === overlayRef.current) onClose();
  };

  return createPortal(
    <div
      ref={overlayRef}
      onClick={handleOverlayClick}
      className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto overscroll-contain bg-black/40 p-2 pt-4 pb-8 sm:p-4 sm:pt-[10vh] sm:pb-12"
    >
      <div className="w-full max-w-3xl">
        <div className="mb-1.5 flex justify-end">
          <button
            onClick={onClose}
            className="flex size-7 cursor-pointer items-center justify-center rounded-full bg-white/90 shadow-md hover:bg-gray-100"
          >
            <X className="size-4 text-gray-600" />
          </button>
        </div>
        {receipt ? (
          <ReceiptCard r={receipt} txHash={txHash} />
        ) : (
          <div className="rounded-lg border border-gray-200 bg-surface p-6 text-center text-sm text-gray-500">
            No matching receipt found
          </div>
        )}
      </div>
    </div>,
    document.body
  );
}
