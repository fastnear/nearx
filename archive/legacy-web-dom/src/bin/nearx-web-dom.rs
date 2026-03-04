#![cfg_attr(target_arch = "wasm32", no_main)]

//! DOM-based Web/Tauri frontend for NEARx.
//!
//! This binary is compiled to WASM and loaded from `web/app.js` via wasm-bindgen.
//! It exposes a minimal JSON-based API:
//!
//!   - `snapshot_json() -> String`
//!   - `handle_action_json(action_json: String) -> String`
//!
//! where `action_json` is a serialized [`UiAction`] and the return value is a
//! serialized [`UiSnapshot`].

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use tokio::sync::mpsc::{error::TryRecvError, unbounded_channel, UnboundedReceiver};
use web_time::{Duration, Instant};

use nearx::ui_snapshot::{apply_ui_action, UiAction, UiSnapshot};
use nearx::{App, AppEvent, Config, Source};

#[cfg(target_arch = "wasm32")]
fn runtime_cfg_string(key: &str) -> Option<String> {
    use wasm_bindgen::JsValue;

    let global = js_sys::global();
    let cfg = js_sys::Reflect::get(&global, &JsValue::from_str("__NEARX_RUNTIME_CONFIG")).ok()?;
    if cfg.is_null() || cfg.is_undefined() {
        return None;
    }

    js_sys::Reflect::get(&cfg, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.trim().is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn runtime_cfg_string(_key: &str) -> Option<String> {
    None
}

/// Wasm-exposed app wrapper. JS owns an instance of this and communicates via JSON.
#[wasm_bindgen]
pub struct WasmApp {
    app: App,
    event_rx: UnboundedReceiver<AppEvent>,
    last_tick: Instant, // For on_tick() throttling
}

impl Default for WasmApp {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmApp {
    /// Construct a new WasmApp and start RPC polling in the background.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmApp {
        console_error_panic_hook::set_once();
        wasm_logger::init(wasm_logger::Config::default());

        // Bootstrap OAuth token from localStorage (if user previously logged in)
        nearx::auth::bootstrap_from_storage();

        // Channel for RPC -> App events.
        let (event_tx, event_rx) = unbounded_channel::<AppEvent>();

        // Render/layout config remains compile-time for web portability.
        let fps: u32 = option_env!("RENDER_FPS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        let fps_choices: Vec<u32> = option_env!("RENDER_FPS_CHOICES")
            .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_else(|| vec![20, 30, 60]);
        let keep_blocks: usize = option_env!("KEEP_BLOCKS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);

        // Read filter configuration from environment variables at compile time
        let default_filter = if let Some(filter) = option_env!("DEFAULT_FILTER") {
            filter.to_string()
        } else if let Some(accounts) = option_env!("WATCH_ACCOUNTS") {
            format!("acct:{}", accounts)
        } else {
            "acct:intents.near".to_string()
        };

        // Runtime overrides from host (Tauri -> worker global):
        // globalThis.__NEARX_RUNTIME_CONFIG = { near_node_url, fastnear_api_url, fastnear_auth_token }
        let runtime_near_node_url = runtime_cfg_string("near_node_url");
        let runtime_fastnear_api_url = runtime_cfg_string("fastnear_api_url");
        let runtime_fastnear_auth_token = runtime_cfg_string("fastnear_auth_token");
        let runtime_archival_rpc_url = runtime_cfg_string("archival_rpc_url");

        let resolved_near_node_url = runtime_near_node_url.unwrap_or_else(|| {
            option_env!("NEAR_NODE_URL")
                .unwrap_or("https://rpc.mainnet.fastnear.com/")
                .to_string()
        });

        let resolved_fastnear_api_url = runtime_fastnear_api_url.unwrap_or_else(|| {
            option_env!("FASTNEAR_API_URL")
                .unwrap_or("https://api.fastnear.com")
                .to_string()
        });

        let resolved_fastnear_auth_token = runtime_fastnear_auth_token.or_else(|| {
            let token = nearx::config::fastnear_token();
            if token.is_empty() {
                None
            } else {
                Some(token)
            }
        });

        let resolved_archival_rpc_url = runtime_archival_rpc_url
            .or_else(|| option_env!("ARCHIVAL_RPC_URL").map(|s| s.to_string()));

        // Initialize archival fetch channel (WASM version)
        let (archival_tx, archival_rx) = unbounded_channel::<u64>();
        let archival_fetch_tx = Some(archival_tx);

        // Initialize tx_details_fetch channel (WASM version): (tx_hash, sender_account_id)
        let (tx_details_tx, tx_details_rx) = unbounded_channel::<(String, String)>();

        // Build config for the RPC poller.
        let cfg_default_filter = default_filter.clone();
        let cfg_fps = fps;
        let cfg_fps_choices = fps_choices.clone();
        let cfg_keep_blocks = keep_blocks;
        let cfg_near_node_url = resolved_near_node_url.clone();
        let cfg_fastnear_api_url = resolved_fastnear_api_url.clone();
        let cfg_fastnear_auth_token = resolved_fastnear_auth_token.clone();
        let cfg_archival_rpc_url = resolved_archival_rpc_url.clone();

        spawn_local(async move {
            let config = Config {
                source: Source::Rpc,
                ws_url: "".to_string(),
                ws_fetch_blocks: false,
                render_fps: cfg_fps,
                render_fps_choices: cfg_fps_choices,
                poll_interval_ms: option_env!("POLL_INTERVAL_MS")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1000),
                poll_max_catchup: 5,
                poll_chunk_concurrency: 4,
                keep_blocks: cfg_keep_blocks,
                near_node_url: cfg_near_node_url,
                near_node_url_explicit: false,
                archival_rpc_url: cfg_archival_rpc_url,
                fastnear_api_url: cfg_fastnear_api_url,
                rpc_timeout_ms: 8_000,
                rpc_retries: 2,
                fastnear_auth_token: cfg_fastnear_auth_token,
                default_filter: cfg_default_filter,
                theme: nearx::theme::Theme::default(),
            };

            log::info!(
                "[WasmApp] RPC poller start - endpoint: {}",
                config.near_node_url
            );

            // Spawn WASM archival fetch task if archival URL configured
            if let Some(archival_url) = config.archival_rpc_url.clone() {
                let auth_token = config.fastnear_auth_token.clone();
                let archival_event_tx = event_tx.clone();

                spawn_local(async move {
                    nearx::archival_fetch_wasm::run_archival_fetch_wasm(
                        archival_rx,
                        archival_event_tx,
                        archival_url,
                        auth_token,
                    )
                    .await;
                });

                log::info!("[WasmApp] Archival fetch task spawned");
            }

            // Spawn WASM tx_details_fetch task (uses RPC with archival fallback)
            {
                let rpc_url = config.near_node_url.clone();
                let archival_url = config.archival_rpc_url.clone();
                let auth_token = config.fastnear_auth_token.clone();
                let tx_details_event_tx = event_tx.clone();

                spawn_local(async move {
                    nearx::tx_details_fetch_wasm::run_tx_details_fetch_wasm(
                        tx_details_rx,
                        tx_details_event_tx,
                        rpc_url,
                        archival_url,
                        auth_token,
                    )
                    .await;
                });

                log::info!(
                    "[WasmApp] Tx details fetch task spawned - RPC: {}, Archival: {:?}, Auth: {}",
                    config.near_node_url,
                    config.archival_rpc_url,
                    if config.fastnear_auth_token.is_some() {
                        "present"
                    } else {
                        "missing"
                    }
                );
            }

            if let Err(e) = nearx::source_rpc::run_rpc(&config, event_tx).await {
                log::error!("[WasmApp] RPC poller error: {e}");
            }
        });

        let app = App::new(
            fps,
            fps_choices,
            keep_blocks,
            default_filter,
            archival_fetch_tx,
            resolved_fastnear_api_url,
            resolved_fastnear_auth_token,
            Some(tx_details_tx),
        );

        WasmApp {
            app,
            event_rx,
            last_tick: Instant::now(),
        }
    }

    /// Get current snapshot as JSON (Rust -> JS).
    #[wasm_bindgen]
    pub fn snapshot_json(&mut self) -> String {
        self.drain_events();
        let snap = UiSnapshot::from_app(&self.app);
        serde_json::to_string(&snap).unwrap_or_else(|e| {
            log::error!("Failed to serialize UiSnapshot: {e}");
            "{}".to_string()
        })
    }

    /// Apply an action (JSON-encoded UiAction) and return an updated snapshot.
    #[wasm_bindgen]
    pub fn handle_action_json(&mut self, action_json: String) -> String {
        self.drain_events();

        match serde_json::from_str::<UiAction>(&action_json) {
            Ok(action) => apply_ui_action(&mut self.app, action),
            Err(e) => {
                log::warn!("[WasmApp] Failed to deserialize UiAction ({e}): {action_json:?}");
            }
        }

        let snap = UiSnapshot::from_app(&self.app);
        serde_json::to_string(&snap).unwrap_or_else(|e| {
            log::error!("Failed to serialize UiSnapshot after action: {e}");
            "{}".to_string()
        })
    }

    /// Apply a deep-link URI using the shared router.
    #[wasm_bindgen(js_name = "applyDeepLink")]
    pub fn apply_deep_link(&mut self, url: String) -> bool {
        self.drain_events();
        if let Some(route) = nearx::router::parse(&url) {
            self.app.apply_route(&route);
            true
        } else {
            false
        }
    }

    // ========================================================================
    // MessagePack API (binary serialization for 3-5x performance improvement)
    // ========================================================================

    /// Get current snapshot as MessagePack binary (optimized for performance).
    ///
    /// Returns binary-encoded UiSnapshot. JavaScript should decode with msgpack.decode().
    /// This is 3-5x faster than JSON serialization and 50-70% smaller payload.
    #[wasm_bindgen]
    pub fn snapshot_msgpack(&mut self) -> Vec<u8> {
        self.drain_events();
        let snap = UiSnapshot::from_app(&self.app);
        snap.to_msgpack()
    }

    /// Apply an action (MessagePack-encoded UiAction) and return an updated snapshot.
    ///
    /// Accepts binary-encoded UiAction, returns binary-encoded UiSnapshot.
    /// Use msgpack.encode() in JavaScript to create the action bytes.
    #[wasm_bindgen]
    pub fn handle_action_msgpack(&mut self, action_bytes: &[u8]) -> Vec<u8> {
        self.drain_events();

        match UiAction::from_msgpack(action_bytes) {
            Ok(action) => apply_ui_action(&mut self.app, action),
            Err(e) => {
                log::warn!("[WasmApp] Failed to deserialize MessagePack UiAction: {e}");
            }
        }

        let snap = UiSnapshot::from_app(&self.app);
        snap.to_msgpack()
    }

    /// Set Details pane viewport size (called by JS based on pane height).
    #[wasm_bindgen(js_name = "setDetailsViewportLines")]
    pub fn set_details_viewport_lines_js(&mut self, lines: u32) {
        self.app.set_details_viewport_lines(lines as usize);
    }

    /// Get clipboard content for the currently focused pane (called only on 'c' key).
    #[wasm_bindgen(js_name = "getClipboardContent")]
    pub fn get_clipboard_content(&mut self) -> String {
        self.drain_events();

        match self.app.pane() {
            0 => self.app.get_raw_block_json(),  // Blocks pane
            1 => self.app.get_raw_tx_json(),     // Transactions pane
            2 => self.app.details().to_string(), // Details pane
            _ => String::new(),
        }
    }
}

/// wasm-bindgen startup hook - applies theme to DOM.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::default());

    // Apply theme CSS vars to :root for TUI-consistent styling
    let theme = nearx::theme::Theme::default();
    apply_theme_to_dom(&theme);
}

#[allow(unused_variables)]
fn apply_theme_to_dom(theme: &nearx::theme::Theme) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use web_sys::{window, HtmlElement};

        if let Some(window) = window() {
            if let Some(document) = window.document() {
                if let Some(root) = document.document_element() {
                    if let Some(html_root) = root.dyn_ref::<HtmlElement>() {
                        let style = html_root.style();
                        for (name, value) in theme.to_css_vars() {
                            let _ = style.set_property(name, &value);
                        }
                        log::info!(
                            "[theme] Applied {} CSS variables to :root",
                            theme.to_css_vars().len()
                        );
                    }
                }
            }
        }
    }
}

// On non-wasm targets this binary is not meant to run; provide a stub main
// so `cargo build --all` remains happy.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("nearx-web-dom is only supported on wasm32-unknown-unknown target.");
}

impl WasmApp {
    fn drain_events(&mut self) {
        // Drain all pending RPC events
        loop {
            match self.event_rx.try_recv() {
                Ok(ev) => self.app.on_event(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    log::warn!("[WasmApp] Event channel disconnected");
                    break;
                }
            }
        }

        // Periodic housekeeping: backfill chain-walking, etc.
        // Call on_tick() at most every 100ms to throttle archival requests
        let now = Instant::now();
        if now.duration_since(self.last_tick) >= Duration::from_millis(100) {
            self.app.on_tick(now);
            self.last_tick = now;
        }
    }
}
