import type { ComponentType } from "react";
import type { TransactionDetail } from "../api/types";
import DefaultTxWidget from "./DefaultTxWidget";
import NearTransferWidget, { matchNearTransfer } from "./NearTransferWidget";
import FtTransferWidget, { matchFtTransfer } from "./FtTransferWidget";

export interface Widget {
  id: string;
  match: (tx: TransactionDetail) => boolean;
  component: ComponentType<{ tx: TransactionDetail }>;
  /** Explanation widgets render above receipts; utility widgets render below */
  type: "explanation" | "utility";
}

/** Order matters — first match of each type wins display position */
const registry: Widget[] = [
  {
    id: "near-transfer",
    match: matchNearTransfer,
    component: NearTransferWidget,
    type: "explanation",
  },
  {
    id: "ft-transfer",
    match: matchFtTransfer,
    component: FtTransferWidget,
    type: "explanation",
  },
  {
    id: "default",
    match: () => true,
    component: DefaultTxWidget,
    type: "utility",
  },
];

export function getMatchingWidgets(tx: TransactionDetail): Widget[] {
  return registry.filter((w) => w.match(tx));
}

export function registerWidget(widget: Widget) {
  registry.push(widget);
}
