import type { TransactionDetail, TransactionAction, ReceiptWithOutcome } from "../api/types";
import { encodeBase58 } from "./format";

export interface ParsedAction {
  type: string;
  method_name?: string;
  deposit?: string;
  args?: string;
  gas?: number;
  public_key?: string;
  access_key_permission?: string;
  beneficiary_id?: string;
  code_hash?: string;
}

export interface TransferInfo {
  from: string | null; // null = minted (no sender)
  to: string | null; // null = burned (no recipient)
  amount: string;
  tokenType: "near" | "nep141" | "nep245";
  contractId?: string; // FT contract for nep141, MT contract for nep245
  tokenId?: string; // raw token_id within NEP-245 contract (e.g. "nep141:wrap.near" or native "42")
}

export interface NftTransferInfo {
  from: string | null;
  to: string | null;
  contractId: string;
  tokenId: string;
}

export interface ParsedTx {
  hash: string;
  signer_id: string;
  receiver_id: string;
  block_height: number;
  timestamp: string;
  gas_burnt: number;
  is_success: boolean | null; // null = pending
  actions: ParsedAction[];
  relayer_id?: string;
  transfers: TransferInfo[];
  nftTransfers: NftTransferInfo[];
  receipts: ReceiptWithOutcome[];
}

export function parseAction(action: TransactionAction): ParsedAction {
  // Actions can be plain strings (e.g. "CreateAccount") or objects (e.g. { Transfer: { deposit: "..." } })
  if (typeof action === "string") {
    return { type: action };
  }
  const type = Object.keys(action)[0];
  const value = action[type];
  const result: ParsedAction = { type };
  if (value && typeof value === "object") {
    const v = value as Record<string, unknown>;
    if (typeof v.method_name === "string") result.method_name = v.method_name;
    if (typeof v.deposit === "string") result.deposit = v.deposit;
    if (typeof v.args === "string") result.args = v.args;
    if (typeof v.gas === "number") result.gas = v.gas;
    if (typeof v.public_key === "string") result.public_key = v.public_key;
    if (typeof v.beneficiary_id === "string") result.beneficiary_id = v.beneficiary_id;
    if (typeof v.code === "string") {
      const bytes = Uint8Array.from(atob(v.code), (c) => c.charCodeAt(0));
      result.code_hash = encodeBase58(bytes);
    }
    if (v.access_key && typeof v.access_key === "object") {
      const ak = v.access_key as Record<string, unknown>;
      if (ak.permission === "FullAccess") {
        result.access_key_permission = "FullAccess";
      } else if (ak.permission && typeof ak.permission === "object") {
        const perm = ak.permission as Record<string, unknown>;
        if (perm.FunctionCall && typeof perm.FunctionCall === "object") {
          const fc = perm.FunctionCall as Record<string, unknown>;
          result.access_key_permission = `FunctionCall → ${fc.receiver_id ?? "?"}`;
        }
      }
    }
  }
  return result;
}

function getDeleteAccountBeneficiary(
  action: TransactionAction,
): string | undefined {
  if (typeof action === "string") return undefined;
  if (action.DeleteAccount && typeof action.DeleteAccount === "object") {
    return (action.DeleteAccount as Record<string, unknown>).beneficiary_id as
      | string
      | undefined;
  }
  if (
    action.type === "DeleteAccount" &&
    typeof action.beneficiary_id === "string"
  ) {
    return action.beneficiary_id as string;
  }
  return undefined;
}

function getTransferDeposit(action: TransactionAction): string | undefined {
  if (typeof action === "string") return undefined;
  // Top-level format: { Transfer: { deposit: "..." } }
  if (action.Transfer && typeof action.Transfer === "object") {
    return (action.Transfer as Record<string, unknown>).deposit as
      | string
      | undefined;
  }
  // Delegate inner format: { type: "Transfer", deposit: "..." }
  if (action.type === "Transfer" && typeof action.deposit === "string") {
    return action.deposit as string;
  }
  return undefined;
}


function extractTransfers(tx: TransactionDetail): TransferInfo[] {
  const transfers: TransferInfo[] = [];

  // Determine real signer/receiver/actions (handling delegates)
  let signer = tx.transaction.signer_id;
  let receiver = tx.transaction.receiver_id;
  let actions: TransactionAction[] = tx.transaction.actions;

  if (actions.length === 1 && actions[0].Delegate) {
    const delegate = actions[0].Delegate as Record<string, unknown>;
    const da = (delegate.delegate_action ?? delegate) as Record<
      string,
      unknown
    >;
    if (typeof da.sender_id === "string") signer = da.sender_id;
    if (typeof da.receiver_id === "string") receiver = da.receiver_id;
    if (Array.isArray(da.actions)) actions = da.actions as TransactionAction[];
  }

  // 1. Native NEAR transfers from actions
  for (const action of actions) {
    const deposit = getTransferDeposit(action);
    if (deposit && deposit !== "0") {
      transfers.push({
        from: signer,
        to: receiver,
        amount: deposit,
        tokenType: "near",
      });
    }
  }

  // 2. DeleteAccount — remaining balance goes to beneficiary
  for (const action of actions) {
    const beneficiary = getDeleteAccountBeneficiary(action);
    if (!beneficiary) continue;
    // Find the system receipt that transfers the remaining balance
    for (const r of tx.receipts) {
      if (
        r.receipt.predecessor_id !== "system" ||
        r.receipt.receiver_id !== beneficiary
      )
        continue;
      const actionData = (r.receipt.receipt as Record<string, unknown>)
        .Action as Record<string, unknown> | undefined;
      const receiptActions =
        (actionData?.actions as TransactionAction[]) ?? [];
      for (const ra of receiptActions) {
        const deposit = getTransferDeposit(ra);
        if (deposit && deposit !== "0") {
          transfers.push({
            from: receiver,
            to: beneficiary,
            amount: deposit,
            tokenType: "near",
          });
        }
      }
    }
  }

  // 3. Event logs from all receipts (NEP-141 and NEP-245)
  for (const r of tx.receipts) {
    const receiptContractId = r.receipt.receiver_id;
    for (const log of r.execution_outcome.outcome.logs) {
      if (!log.startsWith("EVENT_JSON:")) continue;
      try {
        const evt = JSON.parse(log.slice(11));

        // NEP-141 single-token events
        if (evt.standard === "nep141") {
          for (const d of evt.data ?? []) {
            if (!d.amount || d.amount === "0") continue;
            if (evt.event === "ft_transfer") {
              transfers.push({
                from: d.old_owner_id ?? "",
                to: d.new_owner_id ?? "",
                amount: d.amount,
                tokenType: "nep141",
                contractId: receiptContractId,
              });
            } else if (evt.event === "ft_mint") {
              transfers.push({
                from: null,
                to: d.owner_id ?? "",
                amount: d.amount,
                tokenType: "nep141",
                contractId: receiptContractId,
              });
            } else if (evt.event === "ft_burn") {
              transfers.push({
                from: d.owner_id ?? "",
                to: null,
                amount: d.amount,
                tokenType: "nep141",
                contractId: receiptContractId,
              });
            }
          }
        }

        // NEP-245 multi-token events
        if (evt.standard === "nep245") {
          for (const d of evt.data ?? []) {
            const tokenIds: string[] = d.token_ids ?? [];
            const amounts: string[] = d.amounts ?? [];
            for (let j = 0; j < tokenIds.length; j++) {
              const amount = amounts[j];
              if (!amount || amount === "0") continue;
              const rawTokenId = tokenIds[j];
              if (evt.event === "mt_transfer") {
                transfers.push({
                  from: d.old_owner_id ?? "",
                  to: d.new_owner_id ?? "",
                  amount,
                  tokenType: "nep245",
                  contractId: receiptContractId,
                  tokenId: rawTokenId,
                });
              } else if (evt.event === "mt_mint") {
                transfers.push({
                  from: null,
                  to: d.owner_id ?? "",
                  amount,
                  tokenType: "nep245",
                  contractId: receiptContractId,
                  tokenId: rawTokenId,
                });
              } else if (evt.event === "mt_burn") {
                transfers.push({
                  from: d.owner_id ?? "",
                  to: null,
                  amount,
                  tokenType: "nep245",
                  contractId: receiptContractId,
                  tokenId: rawTokenId,
                });
              }
            }
          }
        }
      } catch {
        /* skip malformed event JSON */
      }
    }
  }

  return transfers;
}

function extractNftTransfers(tx: TransactionDetail): NftTransferInfo[] {
  const nftTransfers: NftTransferInfo[] = [];

  for (const r of tx.receipts) {
    const receiptContractId = r.receipt.receiver_id;
    for (const log of r.execution_outcome.outcome.logs) {
      if (!log.startsWith("EVENT_JSON:")) continue;
      try {
        const evt = JSON.parse(log.slice(11));
        if (evt.standard !== "nep171") continue;

        for (const d of evt.data ?? []) {
          const tokenIds: string[] = d.token_ids ?? [];
          for (const tid of tokenIds) {
            if (evt.event === "nft_transfer") {
              nftTransfers.push({
                from: d.old_owner_id ?? "",
                to: d.new_owner_id ?? "",
                contractId: receiptContractId,
                tokenId: tid,
              });
            } else if (evt.event === "nft_mint") {
              nftTransfers.push({
                from: null,
                to: d.owner_id ?? "",
                contractId: receiptContractId,
                tokenId: tid,
              });
            } else if (evt.event === "nft_burn") {
              nftTransfers.push({
                from: d.owner_id ?? "",
                to: null,
                contractId: receiptContractId,
                tokenId: tid,
              });
            }
          }
        }
      } catch {
        /* skip malformed event JSON */
      }
    }
  }

  return nftTransfers;
}

function resolveSuccess(tx: TransactionDetail): boolean | null {
  const { status } = tx.execution_outcome.outcome;
  if ("Failure" in status) return false;
  if ("SuccessValue" in status) return true;
  if ("SuccessReceiptId" in status) {
    const receiptId = status.SuccessReceiptId as string;
    const receipt = tx.receipts.find(
      (r) => r.receipt.receipt_id === receiptId,
    );
    if (!receipt) return null; // receipt not found → pending
    const rStatus = receipt.execution_outcome.outcome.status;
    if ("Failure" in rStatus) return false;
    if ("SuccessValue" in rStatus) return true;
    // SuccessReceiptId at receipt level — treat as success
    if ("SuccessReceiptId" in rStatus) return true;
  }
  return null;
}

export function parseTransaction(tx: TransactionDetail): ParsedTx {
  const is_success = resolveSuccess(tx);

  let totalGas = tx.execution_outcome.outcome.gas_burnt;
  for (const r of tx.receipts) {
    totalGas += r.execution_outcome.outcome.gas_burnt;
  }

  const actions = tx.transaction.actions;
  const parsed: ParsedTx = {
    hash: tx.transaction.hash,
    signer_id: tx.transaction.signer_id,
    receiver_id: tx.transaction.receiver_id,
    block_height: tx.execution_outcome.block_height,
    timestamp: String(tx.execution_outcome.block_timestamp),
    gas_burnt: totalGas,
    is_success,
    actions: actions.map(parseAction),
    transfers: extractTransfers(tx),
    nftTransfers: extractNftTransfers(tx),
    receipts: tx.receipts,
  };

  // Detect Delegate: first action is Delegate wrapping the real signer/receiver
  if (actions.length === 1 && actions[0].Delegate) {
    const delegate = actions[0].Delegate as Record<string, unknown>;
    const delegateAction = (delegate.delegate_action ?? delegate) as Record<
      string,
      unknown
    >;
    if (typeof delegateAction.sender_id === "string") {
      parsed.relayer_id = tx.transaction.signer_id;
      parsed.signer_id = delegateAction.sender_id;
      if (typeof delegateAction.receiver_id === "string") {
        parsed.receiver_id = delegateAction.receiver_id;
      }
      const innerActions = delegateAction.actions;
      if (Array.isArray(innerActions)) {
        parsed.actions = innerActions.map((a: TransactionAction) => {
          // Inner actions may be wrapped as { type: "FunctionCall", ... }
          // or as { FunctionCall: { ... } } like top-level actions
          if (typeof a.type === "string" && a.type !== "object") {
            const result: ParsedAction = { type: a.type as string };
            if (typeof a.method_name === "string")
              result.method_name = a.method_name as string;
            if (typeof a.deposit === "string")
              result.deposit = a.deposit as string;
            if (typeof a.args === "string")
              result.args = a.args as string;
            if (typeof a.gas === "number")
              result.gas = a.gas as number;
            return result;
          }
          return parseAction(a);
        });
      }
    }
  }

  return parsed;
}
