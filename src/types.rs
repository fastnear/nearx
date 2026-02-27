use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsPayload {
    #[serde(rename = "block")]
    Block { data: u64 },
    #[serde(rename = "tx")]
    Tx {
        identifier: Option<String>,
        data: Option<TxSummary>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSummary {
    pub hash: String,
    pub signer: Option<String>,
    pub receiver: Option<String>,
    pub actions: Vec<TxAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxAction {
    pub r#type: String,
    pub method: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockRow {
    #[cfg_attr(
        target_arch = "wasm32",
        serde(serialize_with = "crate::util_text::serialize_u64_as_string")
    )]
    pub height: u64,
    pub hash: String,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(serialize_with = "crate::util_text::serialize_option_u64_as_string")
    )]
    pub prev_height: Option<u64>,
    pub prev_hash: Option<String>,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(serialize_with = "crate::util_text::serialize_u64_as_string")
    )]
    pub timestamp: u64,
    pub tx_count: usize,
    pub when: String,
    pub transactions: Vec<TxLite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxLite {
    pub hash: String,
    // Optional detailed fields (populated when available)
    pub signer_id: Option<String>,
    pub receiver_id: Option<String>,
    pub actions: Option<Vec<ActionSummary>>,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(serialize_with = "crate::util_text::serialize_option_u64_as_string")
    )]
    pub nonce: Option<u64>,
}

/// Rich transaction details parsed from near-primitives
#[derive(Debug, Clone, Serialize)]
pub struct TxDetailed {
    pub hash: String,
    pub signer_id: String,
    pub receiver_id: String,
    pub actions: Vec<ActionSummary>,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(serialize_with = "crate::util_text::serialize_u64_as_string")
    )]
    pub nonce: u64,
    #[allow(dead_code)]
    pub public_key: String,
    #[allow(dead_code)]
    pub raw_transaction: Option<Vec<u8>>, // For debugging/export
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActionSummary {
    CreateAccount,
    DeployContract {
        code_len: usize,
    },
    FunctionCall {
        method_name: String,
        #[serde(skip_serializing)]
        _args_base64: String,
        args_decoded: crate::near_args::DecodedArgs,
        #[cfg_attr(
            target_arch = "wasm32",
            serde(serialize_with = "crate::util_text::serialize_u64_as_string")
        )]
        gas: u64,
        #[cfg_attr(
            target_arch = "wasm32",
            serde(serialize_with = "crate::util_text::serialize_u128_as_string")
        )]
        deposit: u128,
    },
    Transfer {
        #[cfg_attr(
            target_arch = "wasm32",
            serde(serialize_with = "crate::util_text::serialize_u128_as_string")
        )]
        deposit: u128,
    },
    Stake {
        #[cfg_attr(
            target_arch = "wasm32",
            serde(serialize_with = "crate::util_text::serialize_u128_as_string")
        )]
        stake: u128,
        public_key: String,
    },
    AddKey {
        public_key: String,
        access_key: String,
    },
    DeleteKey {
        public_key: String,
    },
    DeleteAccount {
        beneficiary_id: String,
    },
    Delegate {
        sender_id: String,
        receiver_id: String,
        actions: Vec<ActionSummary>,
    },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    FromWs(WsPayload),
    NewBlock(BlockRow),
    FetchedTxDetails { tx_hash: String, json_data: String },
    Quit,
}

/// Jump mark for navigation bookmarks
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mark {
    pub label: String,
    pub pane: u8,
    pub height: Option<u64>,
    pub tx_hash: Option<String>,
    pub when_ms: i64,
    pub pinned: bool,
}
