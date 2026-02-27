use serde::{Deserialize, Serialize};

use crate::{App, InputMode};

/// Block source type for two-list architecture
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiBlockSource {
    Forward,         // Live/cached block from forward list
    BackfillPending, // Backfill slot queued but not yet fetched
    BackfillLoading, // Backfill slot currently being fetched
}

/// One row in the Blocks pane (filtered view).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiBlockRow {
    pub index: usize,
    pub height: u64,
    pub hash: String,
    pub when: String,
    pub tx_count: usize,
    pub available: bool,
    pub is_selected: bool,
    pub source: UiBlockSource, // NEW: tracks whether forward or backfill
}

/// One row in the Transactions pane (filtered view).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTxRow {
    pub index: usize,
    pub hash: String,
    pub signer_id: String,
    pub receiver_id: String,
    pub is_selected: bool,
}

/// DOM-/JSON-/TUI-friendly snapshot of `App` state (Rust → UI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSnapshot {
    /// 0 = Blocks, 1 = Txs, 2 = Details
    pub pane: usize,

    /// Selection slot text (shows current block/tx selection prominently)
    pub selection_slot_text: String,

    /// Current filter text.
    pub filter_query: String,

    /// Whether the filter input is focused (InputMode::Filter).
    pub filter_focused: bool,

    /// Blocks pane rows (filtered + backfill combined).
    pub blocks: Vec<UiBlockRow>,
    pub blocks_total: usize,
    pub blocks_scroll_offset: usize, // NEW: for vertical centering
    pub selected_block_height: Option<u64>,
    pub viewing_cached: bool,

    /// Transactions pane rows (filtered).
    pub txs: Vec<UiTxRow>,
    pub txs_total: usize,

    /// Details pane (windowed JSON for performance)
    pub details: String,
    pub details_scroll: u16,        // Legacy field (line-based now)
    pub details_scroll_line: usize, // Current scroll line (0-based)
    pub details_total_lines: usize, // Total lines in buffer
    pub details_truncated: bool,    // Whether content was truncated at MAX_LINES
    pub details_fullscreen: bool,
    pub fullscreen_mode: String,         // "Scroll" or "Navigate"
    pub fullscreen_content_type: String, // "BlockRawJson", "TransactionRawJson", or "ParsedDetails"

    /// Toast notification text (if any).
    pub toast: Option<String>,

    /// Whether keyboard shortcuts overlay is visible (Web/Tauri render this).
    pub show_shortcuts: bool,

    /// Block height currently being fetched from archival RPC (if any).
    pub loading_block: Option<u64>,
}

impl UiSnapshot {
    /// Build a snapshot from the current app state.
    pub fn from_app(app: &App) -> Self {
        let pane = app.pane();
        let selection_slot_text = app.selection_slot_text();

        // Blocks: forward list (filtered, newest → oldest)
        let (blocks_filtered, selected_block_idx_opt, blocks_total) = app.filtered_blocks();
        let selected_block_height = app.selected_block_height();

        let mut blocks: Vec<UiBlockRow> = blocks_filtered
            .iter()
            .enumerate()
            .map(|(idx, b)| UiBlockRow {
                index: idx,
                height: b.height,
                hash: b.hash.clone(),
                when: b.when.clone(),
                tx_count: b.tx_count,
                available: app.is_block_height_available(b.height),
                is_selected: selected_block_idx_opt == Some(idx),
                source: UiBlockSource::Forward,
            })
            .collect();

        // Blocks: append backfill slots (second list, backwards in time from anchor)
        let loading_block = app.loading_block();
        for slot in app.back_slots() {
            // Skip if block already loaded into forward list/cache
            if app.is_block_available(slot.height) {
                continue;
            }

            let is_loading = loading_block == Some(slot.height);

            blocks.push(UiBlockRow {
                index: blocks.len(), // Continue index sequence
                height: slot.height,
                hash: slot.hash.clone(),
                when: String::new(),
                tx_count: 0,
                available: false,
                is_selected: false, // Placeholders never selected
                source: if is_loading {
                    UiBlockSource::BackfillLoading
                } else {
                    UiBlockSource::BackfillPending
                },
            });
        }

        // Compute scroll offset for vertical centering (like TUI ui.rs:439)
        let viewport_rows = 24; // Reasonable default for web viewport
        let total_rows = blocks.len();
        let mut blocks_scroll_offset = 0;

        if viewport_rows > 0 && total_rows > viewport_rows {
            if let Some(sel_idx) = blocks.iter().position(|r| r.is_selected) {
                let mut offset = sel_idx.saturating_sub(viewport_rows / 2);
                if offset + viewport_rows > total_rows {
                    offset = total_rows.saturating_sub(viewport_rows);
                }
                blocks_scroll_offset = offset;
            }
        }

        let viewing_cached = app.is_viewing_cached_block();

        // Transactions (filtered for current block)
        let (txs_vec, selected_tx_idx, txs_total) = app.txs();
        let txs: Vec<UiTxRow> = txs_vec
            .into_iter()
            .enumerate()
            .map(|(idx, tx)| UiTxRow {
                index: idx,
                hash: tx.hash.clone(),
                signer_id: tx.signer_id.clone().unwrap_or_default(),
                receiver_id: tx.receiver_id.clone().unwrap_or_default(),
                is_selected: idx == selected_tx_idx,
            })
            .collect();

        // Details: use windowed view (prevents UI freeze on huge JSON)
        let details = app.details_window();
        let details_scroll = app.details_scroll(); // Legacy field (line-based now)
        let (details_scroll_line, details_total_lines) = app.details_scroll_info();
        let details_truncated = app.details_truncated();

        let details_fullscreen = app.details_fullscreen();
        let fullscreen_mode = match app.fullscreen_mode() {
            crate::app::FullscreenMode::Scroll => "Scroll".to_string(),
            crate::app::FullscreenMode::Navigate => "Navigate".to_string(),
        };
        let fullscreen_content_type = match app.fullscreen_content_type() {
            crate::app::FullscreenContentType::BlockRawJson => "BlockRawJson".to_string(),
            crate::app::FullscreenContentType::TransactionRawJson => {
                "TransactionRawJson".to_string()
            }
            crate::app::FullscreenContentType::ParsedDetails => "ParsedDetails".to_string(),
        };
        let toast = app.toast_message().map(|s| s.to_string());
        let show_shortcuts = app.show_shortcuts();
        let loading_block = app.loading_block();
        let filter_query = app.filter_query().to_string();
        let filter_focused = app.input_mode() == InputMode::Filter;

        UiSnapshot {
            pane,
            selection_slot_text,
            filter_query,
            filter_focused,
            blocks,
            blocks_total,
            blocks_scroll_offset,
            selected_block_height,
            viewing_cached,
            txs,
            txs_total,
            details,
            details_scroll,
            details_scroll_line,
            details_total_lines,
            details_truncated,
            details_fullscreen,
            fullscreen_mode,
            fullscreen_content_type,
            toast,
            show_shortcuts,
            loading_block,
        }
    }

    /// Serialize to MessagePack binary format (3-5x faster than JSON).
    ///
    /// Returns the binary-encoded snapshot for efficient transfer to web frontend.
    /// Falls back to empty vec on serialization error (should never happen).
    #[cfg(feature = "dom-web")]
    pub fn to_msgpack(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap_or_else(|e| {
            log::error!("Failed to serialize UiSnapshot to MessagePack: {e}");
            Vec::new()
        })
    }

    /// Deserialize from MessagePack binary format.
    ///
    /// For debugging/testing - normally we only serialize snapshots (Rust → JS).
    #[cfg(feature = "dom-web")]
    pub fn from_msgpack(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }
}

/// Frontend-agnostic high-level UI actions (UI → Rust).
///
/// These are what TUI/web/Tauri frontends should send into the core.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UiAction {
    /// Update the filter query (applied immediately).
    SetFilter { text: String },

    /// Focus a pane directly: 0 = Blocks, 1 = Txs, 2 = Details.
    FocusPane { pane: usize },

    /// Select a block row by index in the filtered list.
    SelectBlock { index: usize },

    /// Select a tx row by index in the filtered list.
    SelectTx { index: usize },

    /// Toggle details fullscreen mode.
    ToggleDetailsFullscreen,

    /// Toggle keyboard shortcuts overlay (? key - Web/Tauri only for now).
    ToggleShortcuts,

    /// Keyboard navigation (Arrow keys, PageUp/Down, Tab, Vim keys, etc.).
    Key {
        code: String,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    },

    /// Copy JSON / focused data (pane-aware).
    CopyFocusedJson,
}

impl UiAction {
    /// Deserialize from MessagePack binary format.
    #[cfg(feature = "dom-web")]
    pub fn from_msgpack(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }

    /// Serialize to MessagePack binary format (for testing).
    #[cfg(feature = "dom-web")]
    pub fn to_msgpack(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap_or_else(|e| {
            log::error!("Failed to serialize UiAction to MessagePack: {e}");
            Vec::new()
        })
    }
}

/// Apply a UI action to the core `App`.
///
/// This is where navigation, selection, filters, and copy semantics live.
/// All DOM/TUI frontends should call this for behavior consistency.
pub fn apply_ui_action(app: &mut App, action: UiAction) {
    match action {
        UiAction::SetFilter { text } => {
            app.set_filter_query(text);
        }
        UiAction::FocusPane { pane } => {
            app.set_pane_direct(pane);
        }
        UiAction::SelectBlock { index } => {
            app.select_block_clamped(index);
        }
        UiAction::SelectTx { index } => {
            app.select_tx_clamped(index);
        }
        UiAction::ToggleDetailsFullscreen => {
            app.toggle_details_fullscreen();
        }
        UiAction::ToggleShortcuts => {
            app.toggle_shortcuts();
        }
        UiAction::Key {
            code,
            ctrl,
            alt: _,
            shift,
            meta,
        } => handle_key(app, &code, ctrl || meta, shift),
        UiAction::CopyFocusedJson => handle_copy(app),
    }
}

fn handle_key(app: &mut App, code: &str, _ctrl: bool, shift: bool) {
    // Special handling when Details is fullscreen: arrows scroll the buffer
    if app.details_fullscreen() {
        match code {
            "ArrowUp" | "k" | "K" => {
                app.scroll_details_lines(-1);
                return;
            }
            "ArrowDown" | "j" | "J" => {
                app.scroll_details_lines(1);
                return;
            }
            "PageUp" => {
                let n = app.details_viewport_lines() as isize;
                app.scroll_details_lines(-n);
                return;
            }
            "PageDown" => {
                let n = app.details_viewport_lines() as isize;
                app.scroll_details_lines(n);
                return;
            }
            "Home" => {
                app.details_home();
                return;
            }
            "End" => {
                app.details_end();
                return;
            }
            " " => {
                // Space exits fullscreen
                app.toggle_details_fullscreen();
                return;
            }
            "Escape" => {
                // Esc exits fullscreen
                app.toggle_details_fullscreen();
                return;
            }
            _ => return, // Swallow all other keys in fullscreen
        }
    }

    // Normal (non-fullscreen) handling
    match code {
        // Arrow navigation.
        "ArrowUp" => app.up(),
        "ArrowDown" => app.down(),
        "ArrowLeft" => app.left(),
        "ArrowRight" => app.right(),

        // Vim-style navigation.
        "j" | "J" => app.down(),
        "k" | "K" => app.up(),
        "h" | "H" => app.left(),
        "l" | "L" => app.right(),

        // Paging in details.
        "PageUp" => app.page_up(20),
        "PageDown" => app.page_down(20),

        // Home/End in details.
        "Home" => app.home(),
        "End" => app.end(),

        // Tab / Shift+Tab for pane cycling (BLOCKED in fullscreen to prevent impossible-to-exit state).
        "Tab" if !shift => {
            if !app.details_fullscreen() {
                app.next_pane();
            }
            // Ignore Tab in fullscreen mode
        }
        "Tab" if shift => {
            if !app.details_fullscreen() {
                app.prev_pane();
            }
            // Ignore Shift+Tab in fullscreen mode
        }

        // Esc: priority-based handling (exit fullscreen > clear filter > no-op).
        "Escape" => {
            if app.details_fullscreen() {
                // Priority 1: Exit fullscreen if open
                app.toggle_details_fullscreen();
            } else if !app.filter_query().is_empty() {
                // Priority 2: Clear filter if non-empty
                app.clear_filter();
            }
            // Priority 3: No-op (Esc does nothing if no fullscreen and no filter)
        }

        // Enter: open selected tx into details.
        "Enter" => app.select_tx(),

        // Space: toggle details fullscreen.
        " " => app.toggle_details_fullscreen(),

        // Quit is a no-op for web/Tauri; TUI can layer its own logic.
        "q" | "Q" => {}

        // Everything else: ignore.
        _ => {}
    }
}

fn handle_copy(app: &mut App) {
    if crate::copy_api::copy_current(app) {
        let msg = match app.pane() {
            0 => "Copied block".to_string(),
            1 => "Copied transaction".to_string(),
            2 => "Copied details".to_string(),
            _ => "Copied".to_string(),
        };
        app.show_toast(msg);
    } else {
        app.show_toast("Copy failed".to_string());
    }
}
