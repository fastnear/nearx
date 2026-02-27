//! NEARx - NEAR Blockchain Transaction Viewer
//!
//! This library provides the core functionality for NEARx, a high-performance
//! terminal UI (and web UI) for monitoring NEAR Protocol blockchain transactions.
//!
//! ## Architecture
//!
//! NEARx is built to work in two modes:
//! - **Native**: Terminal UI using crossterm and ratatui
//! - **Web**: DOM-based browser UI with WASM core
//!
//! ## Usage
//!
//! For native builds:
//! ```bash
//! cargo build --features native
//! ```
//!
//! For web builds:
//! ```bash
//! make web-release
//! ```

// Core modules (available on all platforms)
pub mod config;
pub mod constants;
pub mod json_auto_parse;
pub mod json_pretty;
pub mod json_renderer;
pub mod json_syntax;
pub mod types;
pub mod util_text;

// RPC utilities (same direct JSON-RPC implementation for both native and web)
pub mod fastnear_api;
pub mod rpc_utils;

// Theme system (available on all platforms, with platform-specific helpers)
pub mod theme;

// UI core (layout and input policy - available on all platforms)
pub mod ui_core;

// csli-style pane frame helper (native-only)
#[cfg(feature = "native")]
pub mod pane_frame;

pub mod app;
pub mod filter;
pub mod near_args;
pub mod ui;

// Deep link router (available on all platforms)
pub mod router;

// UI feature flags (available on all platforms)
pub mod flags;

// Debug logging system (available on all platforms)
pub mod debug;

// UI snapshot types for DOM-based rendering (all platforms)
pub mod ui_snapshot;

// Pure TUI renderer (draws from UiSnapshot)
pub mod ui_tui_snapshot;

// History module (has native-only implementation internally)
pub mod history;

// Platform-specific modules
#[cfg(feature = "native")]
pub mod source_ws;

#[cfg(feature = "native")]
pub mod archival_fetch;
#[cfg(target_arch = "wasm32")]
pub mod archival_fetch_wasm;

#[cfg(target_arch = "wasm32")]
pub mod tx_details_fetch_wasm;

pub mod source_rpc;

#[cfg(feature = "native")]
pub mod credentials;

#[cfg(feature = "native")]
pub mod marks;

// Platform abstraction layer
pub mod platform;

// Authentication module (web/Tauri JavaScript bridge)
pub mod auth;

// Network utilities (429 backoff for native builds)
#[cfg(feature = "native")]
pub mod net;

// WASM-specific JavaScript bridge (web/Tauri only)
pub mod webshim;

// WASM-facing exports (JS -> Rust) are only built on wasm32.
// Keep surface tight to avoid pulling wasm_bindgen into native targets.
#[cfg(target_arch = "wasm32")]
pub mod wasm_api;

// Utility modules (shared across all targets)
pub mod util;

// Copy functionality (shared across all targets)
pub mod copy_api;
pub mod copy_payload;

// Re-export commonly used types
pub use app::{App, BlockLite, InputMode};
pub use config::{Config, Source};
pub use types::{AppEvent, BlockRow, Mark, TxLite};

// **Stable UI Contract** - All frontends (TUI, Web, Tauri) use these types
// - UiSnapshot: Read-only view of app state (Rust → JS/TUI)
// - UiAction: User input events (JS/TUI → Rust)
// - apply_ui_action: Central dispatcher for all UI actions
// - draw_from_snapshot: Pure TUI renderer (reference layout for DOM)
pub use ui_snapshot::{apply_ui_action, UiAction, UiSnapshot};
pub use ui_tui_snapshot::draw_from_snapshot;
