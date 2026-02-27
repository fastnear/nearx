//! Shared "press `c` to copy" implementation for all targets.
//!
//! This module determines *what* to copy based on which pane is focused
//! and delegates the clipboard write to `platform::copy_to_clipboard`.
//!
//! ## Panes
//!
//! - **0 = Blocks**: Block summary JSON (height, timestamp, all transactions)
//! - **1 = Transactions**: Transaction summary JSON (dual format: chain + human)
//! - **2 = Details**: The JSON content displayed in details pane
//!
//! ## Output Format
//!
//! - Pretty-printed JSON string (human-friendly in chats/issues)
//! - No trailing newline (clipboard-friendly)
//!
//! ## Usage
//!
//! All targets (Native TUI, Web, Tauri) call a single function:
//!
//! ```rust,ignore
//! if copy_api::copy_current(&app) {
//!     // Success - Native TUI shows toast, Web/Tauri may show overlay
//! }
//! ```

use crate::platform;
use crate::App;
use serde_json::Value;

/// Which pane are we copying from?
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CopyPane {
    Blocks,
    Txs,
    Details,
}

/// Determine which pane is currently focused.
#[inline]
pub fn focused_pane(app: &App) -> CopyPane {
    match app.pane() {
        0 => CopyPane::Blocks,
        1 => CopyPane::Txs,
        _ => CopyPane::Details,
    }
}

/// Build the JSON payload for the given pane.
///
/// Returns `None` if there's no content to copy (e.g., no block selected).
pub fn payload_for(app: &mut App, pane: CopyPane) -> Option<Value> {
    match pane {
        CopyPane::Blocks => {
            // Use raw block JSON (same as fullscreen shows)
            let raw_json = app.get_raw_block_json();
            match serde_json::from_str::<Value>(&raw_json) {
                Ok(v) => Some(v),
                Err(_) => {
                    // Not valid JSON (e.g., "Waiting for blocks to load...")
                    // Wrap it as text so copy still works
                    Some(serde_json::json!({ "message": raw_json }))
                }
            }
        }
        CopyPane::Txs => {
            // Use raw transaction JSON (same as fullscreen shows)
            let raw_json = app.get_raw_tx_json();
            match serde_json::from_str::<Value>(&raw_json) {
                Ok(v) => Some(v),
                Err(_) => {
                    // Not valid JSON (e.g., "No transaction selected")
                    // Wrap it as text so copy still works
                    Some(serde_json::json!({ "message": raw_json }))
                }
            }
        }
        CopyPane::Details => {
            // Try to parse the details string as JSON
            let details_str = app.details();
            match serde_json::from_str::<Value>(details_str) {
                Ok(v) => Some(v),
                Err(_) => {
                    // Not valid JSON, wrap it as text
                    Some(serde_json::json!({ "text": details_str }))
                }
            }
        }
    }
}

/// Pretty-print JSON value, without a trailing newline.
#[inline]
fn pretty_no_newline(v: &Value) -> String {
    match serde_json::to_string_pretty(v) {
        Ok(mut s) => {
            if s.ends_with('\n') {
                s.pop();
            }
            s
        }
        Err(_) => String::new(),
    }
}

/// Returns the string that would be copied for the current focus, if any.
///
/// This is useful for testing or preview without actually writing to clipboard.
pub fn current_text(app: &mut App) -> Option<String> {
    let pane = focused_pane(app);
    payload_for(app, pane).map(|v| pretty_no_newline(&v))
}

/// Copies the current pane payload to the clipboard.
///
/// Returns `true` on success, `false` if there's nothing to copy or clipboard operation fails.
///
/// ## Usage in Binaries
///
/// **Native TUI:**
/// ```rust,ignore
/// if copy_api::copy_current(&app) {
///     app.show_toast("Copied".to_string());
/// } else {
///     app.show_toast("Copy failed".to_string());
/// }
/// ```
///
/// **Web/Tauri:**
/// ```rust,ignore
/// // Just call it - platform.js may show overlay on success
/// let _ = copy_api::copy_current(&app);
/// ```
pub fn copy_current(app: &mut App) -> bool {
    match current_text(app) {
        Some(s) if !s.is_empty() => platform::copy_to_clipboard(&s),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focused_pane_mapping() {
        // This test assumes App::new() exists with reasonable defaults
        // In practice, we'd need a proper test app setup
        let app = App::new(
            30,
            vec![30],
            100,
            "".to_string(),
            None,
            "https://tx.main.fastnear.com".to_string(),
            None,
            None,
        );

        // Default pane should be 0 (Blocks)
        assert_eq!(focused_pane(&app), CopyPane::Blocks);
    }

    #[test]
    fn test_pretty_no_newline() {
        let json = serde_json::json!({"test": "value"});
        let result = pretty_no_newline(&json);
        assert!(!result.ends_with('\n'), "Should not have trailing newline");
        assert!(result.contains("\"test\""), "Should contain JSON content");
    }
}
