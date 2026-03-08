use crate::types::{ActionSummary, BlockRow, TxDetailed, TxLite};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::sync::OnceLock;

// Platform-specific imports
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinSet;

#[cfg(not(target_arch = "wasm32"))]
use tokio::time::{sleep, Duration};

#[cfg(target_arch = "wasm32")]
use web_time::Duration;

#[cfg(target_arch = "wasm32")]
use gloo_timers::future::sleep;

static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

pub const NEARX_CLIENT_HEADER: &str = concat!("nearx/", env!("CARGO_PKG_VERSION"));

pub fn http_client() -> &'static reqwest::Client {
    HTTP.get_or_init(|| {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Nearx-Client", NEARX_CLIENT_HEADER.parse().unwrap());

        #[cfg(not(target_arch = "wasm32"))]
        {
            reqwest::Client::builder()
                .default_headers(headers)
                .pool_max_idle_per_host(8)
                .tcp_nodelay(true)
                .build()
                .expect("reqwest client")
        }

        #[cfg(target_arch = "wasm32")]
        {
            reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .expect("reqwest client")
        }
    })
}

pub async fn rpc_post(
    url: &str,
    body: &Value,
    timeout_ms: u64,
    auth_token: Option<&str>,
) -> Result<Value> {
    // Small, bounded retry on transient HTTP failures
    let mut attempt = 0u32;
    loop {
        let request_url = crate::config::with_fastnear_api_key(url, auth_token);
        let req = http_client()
            .post(&request_url)
            .json(body)
            .timeout(Duration::from_millis(timeout_ms));

        if auth_token.is_some() {
            log::debug!("🔑 FastNEAR apiKey query parameter applied");
        } else {
            log::debug!("⚠️ No auth token provided for RPC call");
        }

        let res = req.send().await?;
        if res.status().is_success() {
            let v: Value = res.json().await?;
            if let Some(err) = v.get("error") {
                let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or_default();
                let msg = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("rpc error");
                return Err(anyhow!("rpc {code} {msg}"));
            }
            if let Some(r) = v.get("result") {
                return Ok(r.clone());
            }
            return Err(anyhow!("invalid rpc payload (no result)"));
        } else {
            // Retry only on transient statuses
            if matches!(res.status().as_u16(), 429 | 500 | 502 | 503 | 504) && attempt < 2 {
                attempt += 1;
                sleep(Duration::from_millis(150 * attempt as u64)).await;
                continue;
            }
            return Err(anyhow!("http {}", res.status()));
        }
    }
}

pub async fn get_latest_block(url: &str, t: u64, auth_token: Option<&str>) -> Result<Value> {
    rpc_post(
        url,
        &json!({"jsonrpc":"2.0","id":"nearx","method":"block","params":{"finality":"final"}}),
        t,
        auth_token,
    )
    .await
}

pub async fn get_block_by_height(
    url: &str,
    h: u64,
    t: u64,
    auth_token: Option<&str>,
) -> Result<Value> {
    rpc_post(
        url,
        &json!({"jsonrpc":"2.0","id":"nearx","method":"block","params":{"block_id":h}}),
        t,
        auth_token,
    )
    .await
}

/// Fetch a block by its hash (for canonical chain-walking)
pub async fn get_block_by_hash(
    url: &str,
    hash: &str,
    t: u64,
    auth_token: Option<&str>,
) -> Result<Value> {
    rpc_post(
        url,
        &json!({"jsonrpc":"2.0","id":"nearx","method":"block","params":{"block_id":hash}}),
        t,
        auth_token,
    )
    .await
}

pub async fn get_chunk(url: &str, hash: &str, t: u64, auth_token: Option<&str>) -> Result<Value> {
    rpc_post(
        url,
        &json!({"jsonrpc":"2.0","id":"nearx","method":"chunk","params":{"chunk_id":hash}}),
        t,
        auth_token,
    )
    .await
}

/// Fetch transaction status by hash and sender account ID
///
/// Uses NEAR RPC `tx` method: https://docs.near.org/api/rpc/transactions#transaction-status
///
/// This works on both regular and archival RPC nodes. Use archival RPC URL for old transactions.
pub async fn get_tx_status(
    url: &str,
    tx_hash: &str,
    sender_account_id: &str,
    timeout_ms: u64,
    auth_token: Option<&str>,
) -> Result<Value> {
    rpc_post(
        url,
        &json!({
            "jsonrpc": "2.0",
            "id": "nearx",
            "method": "tx",
            "params": [tx_hash, sender_account_id]
        }),
        timeout_ms,
        auth_token,
    )
    .await
}

/// Extract transactions from a chunk JSON response
fn extract_transactions_from_chunk(chunk: &Value, txs: &mut Vec<TxLite>) {
    if let Some(arr) = chunk["transactions"].as_array() {
        for t in arr {
            // Try to parse full transaction details
            if let Some(detailed) = parse_transaction_detailed(t) {
                txs.push(TxLite {
                    hash: detailed.hash,
                    signer_id: Some(detailed.signer_id),
                    receiver_id: Some(detailed.receiver_id),
                    actions: Some(detailed.actions),
                    nonce: Some(detailed.nonce),
                });
            } else if let Some(hh) = t["hash"].as_str() {
                // Fallback to just hash if parsing fails
                txs.push(TxLite {
                    hash: hh.to_string(),
                    signer_id: None,
                    receiver_id: None,
                    actions: None,
                    nonce: None,
                });
            }
        }
    }
}

pub async fn fetch_block_with_txs(
    url: &str,
    height: u64,
    timeout_ms: u64,
    #[allow(unused_variables)] chunk_concurrency: usize,
    auth_token: Option<&str>,
) -> Result<BlockRow> {
    let b = get_block_by_height(url, height, timeout_ms, auth_token).await?;

    let chunks = b["chunks"].as_array().cloned().unwrap_or_default();
    let mut txs = Vec::<TxLite>::new();

    // Native: Use JoinSet for concurrent chunk fetching
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut set = JoinSet::new();
        for c in chunks.iter() {
            if let Some(hash) = c["chunk_hash"].as_str() {
                let url = url.to_string();
                let hash = hash.to_string();
                let t = timeout_ms;
                let token = auth_token.map(|s| s.to_string());
                set.spawn(async move { get_chunk(&url, &hash, t, token.as_deref()).await });
                if set.len() >= chunk_concurrency.max(1) {
                    let _ = set.join_next().await;
                }
            }
        }

        while let Some(res) = set.join_next().await {
            if let Ok(Ok(chunk)) = res {
                extract_transactions_from_chunk(&chunk, &mut txs);
            }
        }
    }

    // WASM: Sequential chunk fetching (no threads, no Send requirement)
    #[cfg(target_arch = "wasm32")]
    {
        for c in chunks.iter() {
            if let Some(hash) = c["chunk_hash"].as_str() {
                match get_chunk(url, hash, timeout_ms, auth_token).await {
                    Ok(chunk) => extract_transactions_from_chunk(&chunk, &mut txs),
                    Err(e) => log::warn!("Failed to fetch chunk {hash}: {e}"),
                }
            }
        }
    }

    let timestamp = b["header"]["timestamp_nanosec"]
        .as_str()
        .and_then(|s| s.parse::<u128>().ok())
        .unwrap_or(0);

    let when = if timestamp > 0 {
        chrono_fmt(timestamp as i64)
    } else {
        "-".into()
    };

    let hash = b["header"]["hash"].as_str().unwrap_or("").to_string();
    let prev_height = b["header"]["prev_height"].as_u64();
    let prev_hash = b["header"]["prev_hash"].as_str().map(|s| s.to_string());

    Ok(BlockRow {
        height,
        hash,
        prev_height,
        prev_hash,
        timestamp: (timestamp / 1_000_000) as u64,
        tx_count: txs.len(),
        when,
        transactions: txs,
    })
}

fn chrono_fmt(nano: i64) -> String {
    use chrono::{Local, TimeZone, Timelike, Utc};
    let secs = nano / 1_000_000_000;
    let nsec = (nano % 1_000_000_000) as u32;
    let dt = Utc
        .timestamp_opt(secs, nsec)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap());
    let local_dt = dt.with_timezone(&Local);
    format!(
        "{} · {}",
        local_dt.format("%Y-%m-%d %H:%M:%S%.3f"),
        time_of_day_fmt(
            local_dt.hour() as u8,
            local_dt.minute() as u8,
            local_dt.second() as u8
        )
    )
}

fn time_of_day_fmt(h: u8, m: u8, s: u8) -> String {
    let am = h < 12;
    let hour12 = match h % 12 {
        0 => 12,
        x => x as u32,
    };
    format!(
        "{:01}:{:02}:{:02} {}",
        hour12,
        m,
        s,
        if am { "AM" } else { "PM" }
    )
}

/// Parse a transaction from chunk response JSON into TxDetailed
fn parse_transaction_detailed(tx_json: &Value) -> Option<TxDetailed> {
    let hash = tx_json.get("hash")?.as_str()?.to_string();
    let signer_id = tx_json.get("signer_id")?.as_str()?.to_string();
    let receiver_id = tx_json.get("receiver_id")?.as_str()?.to_string();
    let nonce = tx_json.get("nonce").and_then(|n| n.as_u64()).unwrap_or(0);
    let public_key = tx_json
        .get("public_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let actions_array = tx_json.get("actions")?.as_array()?;
    let actions = parse_actions(actions_array);

    Some(TxDetailed {
        hash,
        signer_id,
        receiver_id,
        actions,
        nonce,
        public_key,
        raw_transaction: None,
    })
}

/// Parse actions array from transaction JSON
fn parse_actions(actions_json: &[Value]) -> Vec<ActionSummary> {
    actions_json
        .iter()
        .filter_map(|action| {
            if action.get("CreateAccount").is_some() {
                Some(ActionSummary::CreateAccount)
            } else if let Some(deploy) = action.get("DeployContract") {
                let code_len = deploy
                    .get("code")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                Some(ActionSummary::DeployContract { code_len })
            } else if let Some(fc) = action.get("FunctionCall") {
                let method_name = fc
                    .get("method_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args_base64 = fc
                    .get("args")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let gas = fc.get("gas").and_then(|v| v.as_u64()).unwrap_or(0);
                let deposit = fc
                    .get("deposit")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);

                // Decode args using near_args module
                let args_decoded = crate::near_args::decode_args_base64(
                    if args_base64.is_empty() {
                        None
                    } else {
                        Some(&args_base64)
                    },
                    64, // preview length for binary data
                );

                Some(ActionSummary::FunctionCall {
                    method_name,
                    _args_base64: args_base64,
                    args_decoded,
                    gas,
                    deposit,
                })
            } else if let Some(transfer) = action.get("Transfer") {
                let deposit = transfer
                    .get("deposit")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0u128);
                Some(ActionSummary::Transfer { deposit })
            } else if let Some(stake) = action.get("Stake") {
                let stake_amt = stake
                    .get("stake")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0u128);
                let pk = stake
                    .get("public_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(ActionSummary::Stake {
                    stake: stake_amt,
                    public_key: pk,
                })
            } else if let Some(add_key) = action.get("AddKey") {
                let pk = add_key
                    .get("public_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let ak = if let Some(access_key) = add_key.get("access_key") {
                    access_key.to_string()
                } else {
                    String::new()
                };
                Some(ActionSummary::AddKey {
                    public_key: pk,
                    access_key: ak,
                })
            } else if let Some(del_key) = action.get("DeleteKey") {
                let pk = del_key
                    .get("public_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(ActionSummary::DeleteKey { public_key: pk })
            } else if let Some(del) = action.get("DeleteAccount") {
                let ben = del
                    .get("beneficiary_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(ActionSummary::DeleteAccount {
                    beneficiary_id: ben,
                })
            } else if let Some(delegate) = action.get("Delegate") {
                // Delegate wraps SignedDelegateAction, which has delegate_action field
                let delegate_action = delegate.get("delegate_action").unwrap_or(delegate);
                let sender_id = delegate_action
                    .get("sender_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let receiver_id = delegate_action
                    .get("receiver_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                // Recursively parse nested actions
                let nested_actions = delegate_action
                    .get("actions")
                    .and_then(|a| a.as_array())
                    .map(|arr| parse_actions(arr))
                    .unwrap_or_default();
                Some(ActionSummary::Delegate {
                    sender_id,
                    receiver_id,
                    actions: nested_actions,
                })
            } else {
                None
            }
        })
        .collect()
}
