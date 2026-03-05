import { viewCall, viewAccessKey, broadcastTransaction } from "./rpc";
import { networkId } from "../config";
import { getNearNodeUrl, signTransaction, requestUserPresence } from "../tauri/runtime";

const DEFAULT_RPC_URL =
  networkId === "testnet"
    ? "https://rpc.testnet.fastnear.com"
    : "https://rpc.mainnet.fastnear.com";

export interface ValidatorInfo {
  account_id: string;
  stake: string;
  num_produced_blocks: number;
  num_expected_blocks: number;
}

export interface PoolBalance {
  poolId: string;
  staked: string;
  unstaked: string;
  canWithdraw: boolean | null;
}

export async function getValidators(): Promise<ValidatorInfo[]> {
  const rpcUrl = await getNearNodeUrl(DEFAULT_RPC_URL);
  const res = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "1",
      method: "validators",
      params: [null],
    }),
  });
  const json = await res.json();
  if (json.error) {
    throw new Error(json.error.message ?? JSON.stringify(json.error));
  }
  return json.result.current_validators as ValidatorInfo[];
}

export async function getPoolBalances(
  poolId: string,
  accountId: string,
): Promise<PoolBalance> {
  const [staked, unstaked, canWithdraw] = await Promise.all([
    viewCall<string>(poolId, "get_account_staked_balance", {
      account_id: accountId,
    }).catch(() => "0"),
    viewCall<string>(poolId, "get_account_unstaked_balance", {
      account_id: accountId,
    }).catch(() => "0"),
    viewCall<boolean>(poolId, "is_account_unstaked_balance_available", {
      account_id: accountId,
    }).catch(() => null),
  ]);
  return { poolId, staked, unstaked, canWithdraw };
}

export async function getPoolBalancesBatch(
  poolIds: string[],
  accountId: string,
  onProgress?: (done: number) => void,
): Promise<PoolBalance[]> {
  let done = 0;
  const results = await Promise.all(
    poolIds.map((poolId) =>
      getPoolBalances(poolId, accountId).then((result) => {
        done++;
        onProgress?.(done);
        return result;
      }),
    ),
  );
  return results;
}

export interface StakingActionParams {
  action:
    | "deposit_and_stake"
    | "unstake"
    | "unstake_all"
    | "withdraw"
    | "withdraw_all";
  poolId: string;
  signerId: string;
  publicKey: string;
  amount?: string; // yoctoNEAR
  reason: string;
}

const FIFTY_TGAS = 50_000_000_000_000;

export async function executeStakingAction(
  params: StakingActionParams,
): Promise<{ txHash: string }> {
  const presence = await requestUserPresence(params.reason);
  if (!presence.verified) {
    throw new Error("User presence verification failed — signing aborted");
  }

  const ak = await viewAccessKey(params.signerId, params.publicKey);

  let methodName: string;
  let args: Record<string, unknown>;
  let deposit: string;

  switch (params.action) {
    case "deposit_and_stake":
      methodName = "deposit_and_stake";
      args = {};
      deposit = params.amount ?? "0";
      break;
    case "unstake":
      methodName = "unstake";
      args = { amount: params.amount };
      deposit = "0";
      break;
    case "unstake_all":
      methodName = "unstake_all";
      args = {};
      deposit = "0";
      break;
    case "withdraw":
      methodName = "withdraw";
      args = { amount: params.amount };
      deposit = "0";
      break;
    case "withdraw_all":
      methodName = "withdraw_all";
      args = {};
      deposit = "0";
      break;
  }

  const signResult = await signTransaction({
    signer_id: params.signerId,
    receiver_id: params.poolId,
    nonce: ak.nonce + 1,
    block_hash: ak.block_hash,
    actions: [
      {
        type: "FunctionCall",
        method_name: methodName,
        args: btoa(JSON.stringify(args)),
        gas: FIFTY_TGAS,
        deposit,
      },
    ],
    network: networkId,
    reason: params.reason,
  });

  await broadcastTransaction(signResult.signed_transaction_base64);
  return { txHash: signResult.tx_hash };
}
