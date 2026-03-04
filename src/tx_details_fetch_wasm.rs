// WASM-compatible transaction details fetch task using RPC fallback strategy
#[cfg(target_arch = "wasm32")]
use crate::types::AppEvent;
#[cfg(target_arch = "wasm32")]
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// WASM-compatible background task for fetching transaction details
///
/// Uses NEAR RPC `tx` method with archival fallback:
/// 1. Try regular RPC first (fast, recent transactions)
/// 2. If that fails, try archival RPC (slow, but has full history)
///
/// This requires both tx_hash AND sender_account_id (from the transaction data).
///
/// # Arguments
/// * `fetch_rx` - Channel receiving (tx_hash, sender_account_id) tuples
/// * `event_tx` - Channel for sending fetched transaction details back to app
/// * `rpc_url` - Regular NEAR RPC endpoint URL
/// * `archival_rpc_url` - Archival NEAR RPC endpoint URL (optional)
/// * `auth_token` - Optional auth token for authenticated RPC endpoints
#[cfg(target_arch = "wasm32")]
pub async fn run_tx_details_fetch_wasm(
    mut fetch_rx: UnboundedReceiver<(String, String)>,
    event_tx: UnboundedSender<AppEvent>,
    rpc_url: String,
    archival_rpc_url: Option<String>,
    auth_token: Option<String>,
) {
    web_sys::console::log_1(
        &format!(
            "[TxDetailsFetch][WASM] Starting - RPC: {}, Archival: {}, Auth: {}",
            rpc_url,
            archival_rpc_url.as_deref().unwrap_or("none"),
            if auth_token.is_some() {
                "present"
            } else {
                "missing"
            }
        )
        .into(),
    );

    while let Some((tx_hash, sender_account_id)) = fetch_rx.recv().await {
        let regular_url = rpc_url.clone();
        let archival_url = archival_rpc_url.clone();
        let token = auth_token.clone();
        let tx = event_tx.clone();
        let hash = tx_hash.clone();
        let sender = sender_account_id.clone();

        web_sys::console::log_1(
            &format!(
                "[TxDetailsFetch][WASM] Fetching tx: {} (sender: {})",
                hash, sender
            )
            .into(),
        );

        // Spawn each fetch as independent future (non-blocking)
        spawn_local(async move {
            // Try regular RPC first
            match crate::rpc_utils::get_tx_status(
                &regular_url,
                &hash,
                &sender,
                5000,
                token.as_deref(),
            )
            .await
            {
                Ok(tx_data) => {
                    web_sys::console::log_1(
                        &format!(
                            "[TxDetailsFetch][WASM] ✅ Found tx via regular RPC: {}",
                            hash
                        )
                        .into(),
                    );

                    // Apply auto-parsing to decode EVENT_JSON logs and nested JSON strings
                    let parsed = crate::json_auto_parse::auto_parse_nested_json(tx_data, 5, 0);

                    // Convert to pretty JSON string
                    let json_str = crate::json_pretty::pretty_safe(&parsed, 2, 100 * 1024);

                    // Send back to the app
                    let _ = tx.send(AppEvent::FetchedTxDetails {
                        tx_hash: hash,
                        json_data: json_str,
                    });
                }
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!(
                            "[TxDetailsFetch][WASM] ⚠️ Regular RPC failed: {} - trying archival...",
                            e
                        )
                        .into(),
                    );

                    // Try archival RPC if available
                    if let Some(archival) = archival_url {
                        match crate::rpc_utils::get_tx_status(
                            &archival,
                            &hash,
                            &sender,
                            10000,
                            token.as_deref(),
                        )
                        .await
                        {
                            Ok(tx_data) => {
                                web_sys::console::log_1(
                                    &format!(
                                        "[TxDetailsFetch][WASM] ✅ Found tx via ARCHIVAL RPC: {}",
                                        hash
                                    )
                                    .into(),
                                );

                                // Apply auto-parsing to decode EVENT_JSON logs and nested JSON strings
                                let parsed =
                                    crate::json_auto_parse::auto_parse_nested_json(tx_data, 5, 0);

                                // Convert to pretty JSON string
                                let json_str =
                                    crate::json_pretty::pretty_safe(&parsed, 2, 100 * 1024);

                                // Send back to the app
                                let _ = tx.send(AppEvent::FetchedTxDetails {
                                    tx_hash: hash,
                                    json_data: json_str,
                                });
                            }
                            Err(archival_err) => {
                                web_sys::console::error_1(
                                    &format!(
                                        "[TxDetailsFetch][WASM] ❌ Both RPCs failed for {}: regular={}, archival={}",
                                        hash, e, archival_err
                                    )
                                    .into(),
                                );
                            }
                        }
                    } else {
                        web_sys::console::error_1(
                            &format!("[TxDetailsFetch][WASM] ❌ No archival RPC configured, giving up on {}", hash).into(),
                        );
                    }
                }
            }
        });
    }

    web_sys::console::log_1(&"[TxDetailsFetch][WASM] Channel closed, shutting down".into());
}
