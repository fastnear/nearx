// WASM-compatible archival fetch task (browser fetch API via reqwest-wasm)
#[cfg(target_arch = "wasm32")]
use crate::{types::AppEvent, types::BlockRow};
#[cfg(target_arch = "wasm32")]
use serde_json::json;
#[cfg(target_arch = "wasm32")]
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// WASM-compatible background task for fetching archival blocks
///
/// Unlike the native version, this:
/// - Uses browser Fetch API via reqwest (no blocking I/O)
/// - Spawns each request as a separate future (spawn_local)
/// - Returns immediately if archival_url is None
///
/// # Arguments
/// * `fetch_rx` - Channel receiving block height requests
/// * `block_tx` - Channel for sending fetched blocks back to app
/// * `archival_url` - Archival RPC endpoint URL
/// * `auth_token` - Optional FastNEAR auth token
#[cfg(target_arch = "wasm32")]
pub async fn run_archival_fetch_wasm(
    mut fetch_rx: UnboundedReceiver<u64>,
    block_tx: UnboundedSender<AppEvent>,
    archival_url: String,
    auth_token: Option<String>,
) {
    web_sys::console::log_1(
        &format!("[Archival][WASM] Starting with URL: {}", archival_url).into(),
    );

    while let Some(height) = fetch_rx.recv().await {
        let url = archival_url.clone();
        let token = auth_token.clone();
        let tx = block_tx.clone();

        // Spawn each fetch as independent future (non-blocking)
        spawn_local(async move {
            match fetch_block_from_archival(&url, height, token.as_deref()).await {
                Ok(block) => {
                    web_sys::console::log_1(
                        &format!("[Archival][WASM] ✅ Fetched block #{}", height).into(),
                    );
                    let _ = tx.send(AppEvent::NewBlock(block));
                }
                Err(e) => {
                    web_sys::console::error_1(
                        &format!(
                            "[Archival][WASM] ❌ Failed to fetch block #{}: {}",
                            height, e
                        )
                        .into(),
                    );
                }
            }
        });
    }
}

/// Fetch a single block from archival RPC using browser Fetch API
///
/// Uses reqwest (wasm32 target uses browser fetch under the hood)
#[cfg(target_arch = "wasm32")]
async fn fetch_block_from_archival(
    url: &str,
    height: u64,
    auth_token: Option<&str>,
) -> Result<BlockRow, String> {
    let client = reqwest::Client::new();

    let request_url = crate::config::with_fastnear_api_key(url, auth_token);

    // Build RPC request
    let req = client.post(&request_url).json(&json!({
        "jsonrpc": "2.0",
        "id": "wasm-archival",
        "method": "block",
        "params": {
            "block_id": height
        }
    }));

    // Send request
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    // Check status
    if !resp.status().is_success() {
        return Err(format!(
            "HTTP {}: {}",
            resp.status(),
            resp.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    // Parse JSON response
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse failed: {}", e))?;

    // Check for RPC error
    if let Some(error) = json.get("error") {
        return Err(format!("RPC error: {}", error));
    }

    // Extract result
    let result = json
        .get("result")
        .ok_or_else(|| "No result field in response".to_string())?;

    // Parse block (reuse existing parsing logic)
    parse_block_row_from_rpc(result, height)
}

/// Parse BlockRow from RPC JSON response
///
/// Simplified version of the native parser - focuses on essential fields
#[cfg(target_arch = "wasm32")]
fn parse_block_row_from_rpc(result: &serde_json::Value, height: u64) -> Result<BlockRow, String> {
    let header = result
        .get("header")
        .ok_or_else(|| "No header in block".to_string())?;

    let hash = header
        .get("hash")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp = header
        .get("timestamp")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Parse chunks to get transactions
    let chunks = result
        .get("chunks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "No chunks in block".to_string())?;

    let mut transactions = Vec::new();
    for chunk in chunks {
        if let Some(tx_hash) = chunk.get("tx_root").and_then(|v| v.as_str()) {
            transactions.push(crate::types::TxLite {
                hash: tx_hash.to_string(),
                signer_id: None,   // Not available in block header
                receiver_id: None, // Not available in block header
                actions: None,     // Not available in block header
                nonce: None,       // Not available in block header
            });
        }
    }

    let tx_count = transactions.len();

    // Format timestamp
    let when = if timestamp > 0 {
        let secs = timestamp / 1_000_000_000;
        let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
            .ok_or_else(|| "Invalid timestamp".to_string())?;
        datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        "unknown".to_string()
    };

    // Parse prev_height and prev_hash for canonical chain walking support
    let prev_height = header.get("prev_height").and_then(|v| v.as_u64());
    let prev_hash = header
        .get("prev_hash")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(BlockRow {
        height,
        hash,
        prev_height,
        prev_hash,
        timestamp,
        tx_count,
        when,
        transactions,
    })
}
