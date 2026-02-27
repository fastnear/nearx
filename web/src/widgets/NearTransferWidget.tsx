import type { TransactionDetail } from "../api/types";
import { parseTransaction } from "../utils/parseTransaction";
import AccountId from "../components/AccountId";
import NearAmount from "../components/NearAmount";
import WidgetCard from "../components/WidgetCard";
import { ArrowRight } from "lucide-react";

export function matchNearTransfer(tx: TransactionDetail): boolean {
  const parsed = parseTransaction(tx);
  return (
    parsed.actions.length === 1 &&
    parsed.actions[0].type === "Transfer" &&
    !!parsed.actions[0].deposit
  );
}

export default function NearTransferWidget({
  tx,
}: {
  tx: TransactionDetail;
}) {
  const parsed = parseTransaction(tx);
  const deposit = parsed.actions[0].deposit!;

  return (
    <WidgetCard
      icon={<ArrowRight className="size-4 text-blue-500" />}
      className={
        parsed.is_success === null
          ? "border-yellow-200 bg-yellow-50/50 dark:border-yellow-900 dark:bg-yellow-950/30"
          : parsed.is_success
            ? "border-green-200 bg-green-50/50 dark:border-green-900 dark:bg-green-950/30"
            : "border-red-200 bg-red-50/50 dark:border-red-900 dark:bg-red-950/30"
      }
    >
      <span className="flex flex-wrap items-center gap-1">
        <AccountId accountId={parsed.signer_id} />
        <span className="text-gray-500">transferred</span>
        <span className="font-semibold">
          <NearAmount yoctoNear={deposit} />
        </span>
        <span className="text-gray-500">to</span>
        <AccountId accountId={parsed.receiver_id} />
      </span>
    </WidgetCard>
  );
}
