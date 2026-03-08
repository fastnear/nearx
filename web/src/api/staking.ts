import { viewCall, viewAccessKey, broadcastTransaction } from "./rpc";
import { networkId, nearxHeaders } from "../config";
import { getNearNodeUrl, signTransaction } from "../tauri/runtime";
import type { CredentialSource } from "../tauri/runtime";
import {
  isRetryableHttpStatus,
  isRetryableMessage,
  RetryableRequestError,
  retryAsync,
} from "./retry";

const DEFAULT_RPC_URL =
  networkId === "testnet"
    ? "https://rpc.testnet.fastnear.com"
    : "https://rpc.mainnet.fastnear.com";
const POOL_BALANCE_CONCURRENCY = 4;

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
  const json = await retryAsync(async () => {
    const res = await fetch(rpcUrl, {
      method: "POST",
      headers: nearxHeaders,
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: "1",
        method: "validators",
        params: [null],
      }),
    });
    const text = await res.text();
    const parseJson = () => (text.trim() ? JSON.parse(text) : {});
    if (isRetryableHttpStatus(res.status)) {
      throw new RetryableRequestError(
        text || `RPC error ${res.status}`,
        res.status,
      );
    }
    const parsed = parseJson();
    if (!res.ok) {
      throw new Error(parsed?.error?.message ?? `RPC error ${res.status}`);
    }
    if (parsed.error) {
      const message = parsed.error.message ?? JSON.stringify(parsed.error);
      if (isRetryableMessage(message)) {
        throw new RetryableRequestError(message);
      }
      throw new Error(message);
    }
    return parsed;
  });
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
  let nextIndex = 0;
  const results = new Array<PoolBalance>(poolIds.length);
  const workerCount = Math.min(POOL_BALANCE_CONCURRENCY, poolIds.length);

  async function worker() {
    for (;;) {
      const index = nextIndex++;
      if (index >= poolIds.length) {
        return;
      }
      results[index] = await getPoolBalances(poolIds[index], accountId);
      done++;
      onProgress?.(done);
    }
  }

  await Promise.all(
    Array.from({ length: workerCount }, () => worker()),
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
  credentialSource?: CredentialSource;
  amount?: string; // yoctoNEAR
  reason: string;
}

const FIFTY_TGAS = 50_000_000_000_000;

export class StakingBroadcastError extends Error {
  txHash: string;

  constructor(message: string, txHash: string) {
    super(message);
    this.name = "StakingBroadcastError";
    this.txHash = txHash;
  }
}

export async function executeStakingAction(
  params: StakingActionParams,
): Promise<{ txHash: string; broadcastResult: unknown }> {
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
    signer_public_key: params.publicKey,
    credential_source: params.credentialSource,
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

  try {
    const broadcastResult = await broadcastTransaction(signResult.signed_transaction_base64);
    return { txHash: signResult.tx_hash, broadcastResult };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new StakingBroadcastError(message, signResult.tx_hash);
  }
}
