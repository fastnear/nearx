// /v0/blocks response
export interface BlockHeader {
  block_height: number;
  block_hash: string;
  prev_block_hash: string;
  prev_block_height: number | null;
  block_timestamp: string;
  block_ordinal: number | null;
  gas_price: string;
  gas_burnt: string;
  total_supply: string;
  author_id: string;
  num_transactions: number;
  num_receipts: number;
  chunks_included: number;
  epoch_id: string;
  next_epoch_id: string;
  protocol_version: number;
  tokens_burnt: string;
}

export interface BlocksResponse {
  blocks: BlockHeader[];
}

// /v0/block response
export interface BlockTx {
  transaction_hash: string;
  signer_id: string;
  receiver_id: string;
  real_signer_id: string;
  real_receiver_id: string;
  tx_block_height: number;
  tx_block_timestamp: string;
  tx_index: number;
  gas_burnt: number;
  is_success: boolean;
  is_completed: boolean;
  is_relayed: boolean;
  tokens_burnt: string;
  shard_id: number;
  nonce: number;
  priority_fee: number;
  signer_public_key: string;
  tx_block_hash: string;
  last_block_height: number;
}

export interface BlockDetailResponse {
  block: BlockHeader;
  block_txs: BlockTx[];
}

// /v0/transactions response — deeply nested RPC-like structure
export interface TransactionDetail {
  transaction: {
    signer_id: string;
    receiver_id: string;
    hash: string;
    actions: TransactionAction[];
    [key: string]: unknown;
  };
  execution_outcome: {
    block_hash: string;
    block_height: number;
    block_timestamp: number;
    id: string;
    outcome: ExecutionOutcome;
  };
  receipts: ReceiptWithOutcome[];
  data_receipts: unknown[];
}

export interface TransactionAction {
  [key: string]: unknown;
}

export interface ExecutionOutcome {
  executor_id: string;
  gas_burnt: number;
  logs: string[];
  receipt_ids: string[];
  status: Record<string, unknown>;
  tokens_burnt: string;
  metadata?: unknown;
}

export interface ReceiptWithOutcome {
  receipt: {
    block_hash: string;
    block_height: number;
    block_timestamp: number;
    predecessor_id: string;
    receiver_id: string;
    receipt_id: string;
    receipt: Record<string, unknown>;
    [key: string]: unknown;
  };
  execution_outcome: {
    block_hash: string;
    block_height: number;
    block_timestamp: number;
    id: string;
    outcome: ExecutionOutcome;
  };
}

export interface TransactionsResponse {
  transactions: TransactionDetail[];
}

// /v0/account response
export interface AccountTx {
  account_id: string;
  transaction_hash: string;
  tx_block_height: number;
  tx_block_timestamp: string;
  tx_index: number;
  is_success: boolean;
  is_signer: boolean;
  is_receiver: boolean;
  is_real_signer: boolean;
  is_real_receiver: boolean;
  is_predecessor: boolean;
  is_function_call: boolean;
  is_any_signer: boolean;
  is_delegated_signer: boolean;
  is_event_log: boolean;
  is_action_arg: boolean;
  is_explicit_refund_to: boolean;
}

export interface AccountResponse {
  account_txs: AccountTx[];
  resume_token?: string;
  txs_count: number;
}

export interface BlockFilters {
  from_block_height?: number;
  to_block_height?: number;
  desc?: boolean;
}

export interface AccountFilters {
  is_signer?: boolean;
  is_delegated_signer?: boolean;
  is_real_signer?: boolean;
  is_any_signer?: boolean;
  is_predecessor?: boolean;
  is_explicit_refund_to?: boolean;
  is_receiver?: boolean;
  is_real_receiver?: boolean;
  is_function_call?: boolean;
  is_action_arg?: boolean;
  is_event_log?: boolean;
  is_success?: boolean;
  from_tx_block_height?: number;
  to_tx_block_height?: number;
  desc?: boolean;
}
