import type { TransactionDetail } from "../api/types";
import { parseTransaction } from "../utils/parseTransaction";
import useTokenMetadata from "../hooks/useTokenMetadata";
import AccountId from "../components/AccountId";
import { TokenAmount } from "../components/TransferSummary";
import WidgetCard from "../components/WidgetCard";
import { ArrowRight, Loader2 } from "lucide-react";

interface FtTransferArgs {
  receiver_id: string;
  amount: string;
  msg?: string;
}

function decodeFtArgs(base64: string): FtTransferArgs | null {
  try {
    const text = atob(base64);
    return JSON.parse(text) as FtTransferArgs;
  } catch {
    return null;
  }
}

export function matchFtTransfer(tx: TransactionDetail): boolean {
  const parsed = parseTransaction(tx);
  if (parsed.actions.length !== 1) return false;
  const action = parsed.actions[0];
  return (
    action.type === "FunctionCall" &&
    (action.method_name === "ft_transfer" ||
      action.method_name === "ft_transfer_call") &&
    !!action.args
  );
}

export default function FtTransferWidget({
  tx,
}: {
  tx: TransactionDetail;
}) {
  const parsed = parseTransaction(tx);
  const action = parsed.actions[0];
  const tokenContractId = parsed.receiver_id;
  const { metadata, loading } = useTokenMetadata(tokenContractId);

  const args = action.args ? decodeFtArgs(action.args) : null;

  if (!args) return null;

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
          {loading ? (
            <Loader2 className="inline size-3.5 animate-spin text-gray-400" />
          ) : metadata ? (
            <TokenAmount amount={args.amount} meta={metadata} contractId={tokenContractId} />
          ) : (
            <>
              {args.amount}{" "}
              <AccountId accountId={tokenContractId} />
            </>
          )}
        </span>
        <span className="text-gray-500">to</span>
        <AccountId accountId={args.receiver_id} />
      </span>
    </WidgetCard>
  );
}
