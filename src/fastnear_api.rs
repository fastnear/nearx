use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::time::Duration;

/// Fetch full transaction details from FastNEAR Explorer API
///
/// The API returns comprehensive transaction data including:
/// - Full execution outcomes
/// - Gas burnt and tokens burnt
/// - Receipt information
/// - Transaction status details
pub async fn fetch_transaction_details(
    api_url: &str,
    tx_hash: &str,
    timeout_ms: u64,
    auth_token: Option<&str>,
) -> Result<Value> {
    let client = crate::rpc_utils::http_client();

    // FastNEAR API expects an array of tx hashes (max 20)
    let body = json!({
        "tx_hashes": [tx_hash]
    });

    log::info!(
        "[fastnear_api] Fetching transaction details for {}",
        tx_hash
    );

    let endpoint =
        crate::config::with_fastnear_api_key(&format!("{}/v0/transactions", api_url), auth_token);
    let request = client
        .post(endpoint)
        .json(&body)
        .timeout(Duration::from_millis(timeout_ms));

    if auth_token.is_some() {
        log::debug!("[fastnear_api] Using FastNEAR apiKey query parameter");
    }

    let response = request
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch from FastNEAR API: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow!("FastNEAR API error ({}): {}", status, error_text));
    }

    let data: Value = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse FastNEAR response: {}", e))?;

    log::debug!("[fastnear_api] Response structure: {:?}", data);

    // Extract first transaction from the response array
    if let Some(txs) = data["transactions"].as_array() {
        log::info!(
            "[fastnear_api] Found {} transactions in response",
            txs.len()
        );
        if let Some(tx) = txs.first() {
            // Log transaction structure preview
            let tx_preview = if tx.is_object() {
                format!(
                    "Object with {} keys",
                    tx.as_object().map(|o| o.len()).unwrap_or(0)
                )
            } else {
                "Non-object response".to_string()
            };
            log::info!(
                "[fastnear_api] Successfully fetched transaction details: {}",
                tx_preview
            );
            return Ok(tx.clone());
        }
    }

    log::warn!(
        "[fastnear_api] No transactions found in response. Data structure: {:?}",
        data.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
    Err(anyhow!("Transaction not found in FastNEAR response"))
}

/// Batch fetch multiple transactions (up to 20 at once)
/// Returns a map of tx_hash -> transaction data
pub async fn fetch_transactions_batch(
    api_url: &str,
    tx_hashes: &[String],
    timeout_ms: u64,
    auth_token: Option<&str>,
) -> Result<std::collections::HashMap<String, Value>> {
    if tx_hashes.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // API limit is 20 transactions per request
    if tx_hashes.len() > 20 {
        return Err(anyhow!("Cannot fetch more than 20 transactions at once"));
    }

    let client = crate::rpc_utils::http_client();

    let body = json!({
        "tx_hashes": tx_hashes
    });

    log::info!(
        "[fastnear_api] Batch fetching {} transactions",
        tx_hashes.len()
    );

    let endpoint =
        crate::config::with_fastnear_api_key(&format!("{}/v0/transactions", api_url), auth_token);
    let request = client
        .post(endpoint)
        .json(&body)
        .timeout(Duration::from_millis(timeout_ms));

    if auth_token.is_some() {
        log::debug!("[fastnear_api] Using FastNEAR apiKey query parameter");
    }

    let response = request
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch from FastNEAR API: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow!("FastNEAR API error ({}): {}", status, error_text));
    }

    let data: Value = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse FastNEAR response: {}", e))?;

    let mut result = std::collections::HashMap::new();

    if let Some(txs) = data["transactions"].as_array() {
        for tx in txs {
            if let Some(hash) = tx["hash"].as_str() {
                result.insert(hash.to_string(), tx.clone());
            }
        }
    }

    log::info!(
        "[fastnear_api] Successfully fetched {} transactions",
        result.len()
    );
    Ok(result)
}
