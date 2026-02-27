// Native-only archival fetch task (uses tokio full runtime + blocking I/O)
#[cfg(feature = "native")]
use crate::{config::Config, rpc_utils::fetch_block_with_txs, types::AppEvent};
#[cfg(feature = "native")]
use anyhow::Result;
#[cfg(feature = "native")]
use tokio::sync::mpsc::UnboundedReceiver;

/// Background task that fetches historical blocks from archival RPC endpoint
/// Receives block height requests and fetches them on demand
#[cfg(feature = "native")]
pub async fn run_archival_fetch(
    cfg: Config,
    mut fetch_rx: UnboundedReceiver<u64>,
    block_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    // Must have archival URL configured
    let archival_url = match &cfg.archival_rpc_url {
        Some(url) => url.clone(),
        None => return Ok(()), // No archival URL, exit immediately
    };

    log::debug!("[Archival] Starting archival fetch task with URL: {archival_url}");

    // Get effective auth token with priority: User token (from auth module) → Config token → None
    let get_token = || -> Option<String> {
        // Try user token first (from authenticated login via auth module)
        if let Some(token) = crate::auth::token_string() {
            log::debug!("[Archival] Using user FastNEAR token (from auth)");
            return Some(token);
        }
        // Fall back to config token (from env or URL param)
        if let Some(ref token) = cfg.fastnear_auth_token {
            log::debug!("[Archival] Using config FastNEAR token (env/URL)");
            Some(token.clone())
        } else {
            log::warn!("[Archival] No FastNEAR token (may hit rate limits on archival endpoint)");
            None
        }
    };

    while let Some(height) = fetch_rx.recv().await {
        log::debug!("[Archival] Received request to fetch block #{height}");

        let token = get_token(); // Get current token (may have been updated)

        match fetch_block_with_txs(
            &archival_url,
            height,
            cfg.rpc_timeout_ms,
            cfg.poll_chunk_concurrency,
            token.as_deref(),
        )
        .await
        {
            Ok(block) => {
                log::info!(
                    "[Archival] Successfully fetched block #{} ({} txs)",
                    height,
                    block.tx_count
                );
                // Send block via existing event channel
                if let Err(e) = block_tx.send(AppEvent::NewBlock(block)) {
                    log::error!("[Archival] Failed to send block: {e}");
                    break;
                }
            }
            Err(e) => {
                log::error!("[Archival] Failed to fetch block #{height}: {e}");
                // TODO: Send error event to App so it can show toast notification
                // For now, just log the error
            }
        }
    }

    log::debug!("[Archival] Archival fetch task shutting down");
    Ok(())
}
