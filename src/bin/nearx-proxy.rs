//! Backend proxy server for NEARx web frontend
//!
//! This server provides a lightweight HTTP API that wraps NEAR RPC calls,
//! allowing the WASM frontend to access blockchain data without dealing with
//! C dependency constraints (near-crypto, near-primitives).
//!
//! ## Endpoints
//! - GET /health - Health check
//! - POST /rpc - Generic JSON-RPC proxy (auto-injects auth token)
//! - GET /api/latest - Get latest finalized block height
//! - GET /api/block/:height - Fetch block with all chunks and transactions
//! - GET /api/blocks?from=N&limit=M - Batch fetch blocks (for initial load)
//!
//! ## Usage
//! ```bash
//! cargo run --bin nearx-proxy --features proxy
//! ```

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use nearx::{
    rpc_utils::{fetch_block_with_txs, NEARX_CLIENT_HEADER},
    types::BlockRow,
};

fn nearx_http_client() -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("X-Nearx-Client", NEARX_CLIENT_HEADER.parse().unwrap());
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .expect("reqwest client")
}

/// Application state shared across handlers
#[derive(Clone)]
struct AppState {
    rpc_url: String,
    auth_token: Option<String>,
    timeout_ms: u64,
    chunk_concurrency: usize,
}

/// Query parameters for batch block fetching
#[derive(Debug, Deserialize)]
struct BlocksQuery {
    from: u64,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    100
}

/// Response for /api/latest endpoint
#[derive(Debug, Serialize)]
struct LatestResponse {
    height: u64,
}

/// Response for /api/blocks batch endpoint
#[derive(Debug, Serialize)]
struct BlocksResponse {
    blocks: Vec<BlockRow>,
    from: u64,
    count: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Load configuration (mainnet by default)
    let rpc_url = std::env::var("NEAR_NODE_URL")
        .unwrap_or_else(|_| "https://rpc.mainnet.fastnear.com/".to_string());

    let auth_token = nearx::config::fastnear_env_token();

    let timeout_ms = std::env::var("RPC_TIMEOUT_MS")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000);

    let chunk_concurrency = std::env::var("POLL_CHUNK_CONCURRENCY")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4);

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3030);

    log::info!("🦀 NEARx Proxy Server");
    log::info!("NEAR RPC: {}", rpc_url);
    log::info!(
        "API key: {}",
        if auth_token.is_some() {
            "configured"
        } else {
            "none"
        }
    );
    log::info!("RPC timeout: {}ms", timeout_ms);
    log::info!("Chunk concurrency: {}", chunk_concurrency);
    log::info!("Port: {}", port);

    // Configure CORS (allow all origins for now, tighten in production)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    // Build application state
    let state = AppState {
        rpc_url,
        auth_token,
        timeout_ms,
        chunk_concurrency,
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/rpc", post(rpc_proxy_handler))
        .route("/api/latest", get(get_latest_handler))
        .route("/api/block/:height", get(get_block_handler))
        .route("/api/blocks", get(get_blocks_batch_handler))
        .layer(cors)
        .with_state(state);

    // Start server
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log::info!("🚀 Server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health_handler() -> &'static str {
    "OK"
}

/// Generic JSON-RPC proxy endpoint
/// Forwards any JSON-RPC request to NEAR RPC with auto-injected auth token
async fn rpc_proxy_handler(
    State(state): State<AppState>,
    body: String,
) -> Result<Response<Body>, StatusCode> {
    log::debug!("Proxying JSON-RPC request");

    // Parse the incoming JSON to validate it
    let _json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        log::error!("Invalid JSON in request body: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    let client = nearx_http_client();

    // Build request with optional FastNEAR apiKey query parameter
    let request_url =
        nearx::config::with_fastnear_api_key(&state.rpc_url, state.auth_token.as_deref());
    let req = client
        .post(&request_url)
        .header("Content-Type", "application/json")
        .body(body);

    // Forward request to NEAR RPC
    let resp = req.send().await.map_err(|e| {
        log::error!("Failed to forward RPC request: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    // Get response status and body
    let status = resp.status();
    let body_bytes = resp.bytes().await.map_err(|e| {
        log::error!("Failed to read RPC response: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    // Build response with same status code
    let response = Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(body_bytes))
        .map_err(|e| {
            log::error!("Failed to build response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    log::debug!("RPC proxy response: status={}", status);

    Ok(response)
}

/// Get latest finalized block height
async fn get_latest_handler(
    State(state): State<AppState>,
) -> Result<Json<LatestResponse>, StatusCode> {
    log::debug!("Fetching latest block height");

    let client = nearx_http_client();

    let request_url =
        nearx::config::with_fastnear_api_key(&state.rpc_url, state.auth_token.as_deref());
    let req = client.post(&request_url).json(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": "latest",
        "method": "block",
        "params": {
            "finality": "final"
        }
    }));

    let resp = req.send().await.map_err(|e| {
        log::error!("Failed to fetch latest block: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        log::error!("Failed to parse latest block response: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // FastNEAR sometimes returns height as Number, sometimes as String
    let height = json["result"]["header"]["height"]
        .as_u64()
        .or_else(|| {
            json["result"]["header"]["height"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
        })
        .ok_or_else(|| {
            log::error!("Missing or invalid height in response: {:?}", json);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    log::debug!("Latest block height: {}", height);

    Ok(Json(LatestResponse { height }))
}

/// Fetch a single block with all chunks and transactions
async fn get_block_handler(
    Path(height): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<BlockRow>, StatusCode> {
    log::debug!("Fetching block at height {}", height);

    let block = fetch_block_with_txs(
        &state.rpc_url,
        height,
        state.timeout_ms,
        state.chunk_concurrency,
        state.auth_token.as_deref(),
    )
    .await
    .map_err(|e| {
        log::error!("Failed to fetch block {}: {}", height, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    log::debug!(
        "Fetched block {} with {} txs",
        height,
        block.transactions.len()
    );

    Ok(Json(block))
}

/// Batch fetch multiple blocks (for initial load)
async fn get_blocks_batch_handler(
    Query(params): Query<BlocksQuery>,
    State(state): State<AppState>,
) -> Result<Json<BlocksResponse>, StatusCode> {
    let limit = params.limit.min(200); // Cap at 200 blocks per request
    log::debug!("Batch fetching {} blocks from {}", limit, params.from);

    let mut blocks = Vec::with_capacity(limit);

    // Fetch blocks in descending order (newest first)
    for offset in 0..limit {
        let height = params.from.saturating_sub(offset as u64);

        match fetch_block_with_txs(
            &state.rpc_url,
            height,
            state.timeout_ms,
            state.chunk_concurrency,
            state.auth_token.as_deref(),
        )
        .await
        {
            Ok(block) => blocks.push(block),
            Err(e) => {
                log::warn!("Failed to fetch block {} in batch: {}", height, e);
                // Continue with other blocks instead of failing entire batch
            }
        }
    }

    let count = blocks.len();
    log::debug!("Batch fetch complete: {}/{} blocks", count, limit);

    Ok(Json(BlocksResponse {
        blocks,
        from: params.from,
        count,
    }))
}
