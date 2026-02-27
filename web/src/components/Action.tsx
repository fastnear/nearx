import {
  Code2,
  ArrowRightLeft,
  Key,
  UserPlus,
  UserMinus,
  FileCode,
  Coins,
  Fuel,
  type LucideIcon,
} from "lucide-react";
import type { ParsedAction } from "../utils/parseTransaction";
import AccountId from "./AccountId";
import Base64Data from "./Base64Data";
import GasAmount from "./GasAmount";
import NearAmount from "./NearAmount";

const iconClass = "inline-block size-3.5 text-gray-400";

function Deposit({ deposit }: { deposit?: string }) {
  if (!deposit || deposit === "0") return null;
  return <span className="ml-1 text-gray-500" onClick={(e) => e.stopPropagation()}><NearAmount yoctoNear={deposit} /></span>;
}

function ActionFunctionCall({ action }: { action: ParsedAction }) {
  return (
    <span className="inline-flex items-center gap-1 whitespace-nowrap">
      <Code2 className={iconClass} />
      <span>{action.method_name ?? "call"}</span>
      <Deposit deposit={action.deposit} />
    </span>
  );
}

function ActionTransfer({ action }: { action: ParsedAction }) {
  return (
    <span className="inline-flex items-center gap-1 whitespace-nowrap">
      <ArrowRightLeft className={iconClass} />
      <span>Transfer</span>
      <Deposit deposit={action.deposit} />
    </span>
  );
}

const genericIcons: Record<string, LucideIcon> = {
  CreateAccount: UserPlus,
  DeleteAccount: UserMinus,
  AddKey: Key,
  DeleteKey: Key,
  Stake: Coins,
  DeployContract: FileCode,
};

function ActionGeneric({ action }: { action: ParsedAction }) {
  const Icon = genericIcons[action.type];
  return (
    <span className="inline-flex items-center gap-1 whitespace-nowrap">
      {Icon && <Icon className={iconClass} />}
      <span>{action.type}</span>
    </span>
  );
}

export default function Action({ action }: { action: ParsedAction }) {
  switch (action.type) {
    case "FunctionCall":
      return <ActionFunctionCall action={action} />;
    case "Transfer":
      return <ActionTransfer action={action} />;
    default:
      return <ActionGeneric action={action} />;
  }
}

function ActionKeyExpanded({ action }: { action: ParsedAction }) {
  return (
    <div className="w-full text-sm space-y-1">
      <span className="flex items-center gap-1 font-medium">
        <Key className={iconClass} />
        <span>{action.type}</span>
        {action.access_key_permission && (
          <span className="font-normal text-gray-500">({action.access_key_permission})</span>
        )}
      </span>
      {action.public_key && (
        <div className="font-mono text-xs text-gray-400 break-all">{action.public_key}</div>
      )}
    </div>
  );
}

function ActionDeployContractExpanded({ action }: { action: ParsedAction }) {
  return (
    <div className="w-full text-sm space-y-1">
      <span className="flex items-center gap-1 font-medium">
        <FileCode className={iconClass} />
        <span>DeployContract</span>
      </span>
      {action.code_hash && (
        <div className="font-mono text-xs text-gray-400 break-all">{action.code_hash}</div>
      )}
    </div>
  );
}

function ActionDeleteAccountExpanded({ action }: { action: ParsedAction }) {
  return (
    <div className="w-full text-sm space-y-1">
      <span className="flex items-center gap-1 font-medium">
        <UserMinus className={iconClass} />
        <span>DeleteAccount</span>
      </span>
      {action.beneficiary_id && (
        <div className="text-xs text-gray-500">
          Beneficiary: <AccountId accountId={action.beneficiary_id} />
        </div>
      )}
    </div>
  );
}

export function ActionExpanded({ action }: { action: ParsedAction }) {
  if (action.type === "AddKey" || action.type === "DeleteKey") {
    return <ActionKeyExpanded action={action} />;
  }
  if (action.type === "DeleteAccount") {
    return <ActionDeleteAccountExpanded action={action} />;
  }
  if (action.type === "DeployContract") {
    return <ActionDeployContractExpanded action={action} />;
  }
  if (action.type !== "FunctionCall") {
    return <div><Action action={action} /></div>;
  }

  return (
    <div className="w-full text-sm space-y-1">
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-1 font-medium">
          <Code2 className={iconClass} />
          <span>{action.method_name ?? "call"}</span>
        </span>
        {action.gas != null && (
          <span className="flex items-center gap-1 text-xs text-gray-500">
            <Fuel className="size-3 text-gray-400" />
            <GasAmount gas={action.gas} />
          </span>
        )}
        {action.deposit && action.deposit !== "0" && (
          <span className="text-xs text-gray-500">
            <NearAmount yoctoNear={action.deposit} />
          </span>
        )}
      </div>
      {action.args && <Base64Data base64={action.args} />}
    </div>
  );
}
