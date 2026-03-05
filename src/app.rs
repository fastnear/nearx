use serde_json::json;
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};

#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::filter::{self, compile_filter, tx_matches_filter, CompiledFilter};
use crate::flags::UiFlags;
use crate::json_pretty::pretty;
use crate::theme::Theme;
use crate::types::{AppEvent, BlockRow, TxLite, WsPayload};

#[cfg(feature = "native")]
use crate::theme::ratatui_helpers;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    Search,
    Marks,
}

/// Content type for fullscreen Details pane
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FullscreenContentType {
    BlockRawJson,       // Raw JSON of selected block (from Blocks pane)
    TransactionRawJson, // Raw JSON of selected transaction (from Txs pane)
    ParsedDetails,      // Human-readable parsed view (from Details pane, default)
}

/// Interaction mode when fullscreen is active
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FullscreenMode {
    Scroll,   // Arrow keys scroll the JSON content
    Navigate, // Arrow keys navigate underlying pane (Blocks/Txs rows)
}

/// Reason for block selection change - determines tx selection behavior
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum BlockChangeReason {
    AutoFollow,   // New block arrived in auto-follow mode (keep tx index)
    ManualNav,    // User manually navigated to different block (reset tx)
    FilterChange, // Filter was applied/cleared (try to preserve tx)
}

const BACK_WINDOW: usize = 50;
const FRONT_WINDOW: u64 = 50;

/// Maximum number of full transaction JSONs to cache (256 * ~100KB = ~25MB worst case)
const MAX_FULL_TX: usize = 256;

/// Backwards-fill slot for the block list (ancestors of the anchor block).
#[derive(Debug, Clone)]
pub struct BackSlot {
    pub height: u64,
    pub hash: String,
    pub state: BackSlotState,
}

#[derive(Debug, Clone)]
pub enum BackSlotState {
    /// We know this height/hash but have not yet asked the archival worker.
    Pending,
    /// Archival worker has delivered this height (visible via `is_block_available`).
    Loaded,
    Error(String),
}

/// Virtual text buffer for Details pane with windowed rendering.
/// Stores full JSON and line offsets for efficient scrolling.
pub struct DetailsBuffer {
    /// Full pretty-printed JSON (or other text)
    text: String,
    /// Starting byte index of each line in `text`
    line_offsets: Vec<usize>,
    /// Current top visible line
    scroll_line: usize,
    /// Whether the content was truncated at MAX_LINES
    truncated: bool,
}

impl Default for DetailsBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl DetailsBuffer {
    /// Maximum lines to index (prevents UI freeze on massive blocks)
    const MAX_LINES: usize = 5_000;

    pub fn new() -> Self {
        Self {
            text: String::new(),
            line_offsets: vec![0],
            scroll_line: 0,
            truncated: false,
        }
    }

    /// Replace buffer contents and rebuild line index
    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.line_offsets.clear();
        self.line_offsets.push(0);

        let mut line_count = 1;
        for (i, b) in self.text.bytes().enumerate() {
            if b == b'\n' {
                self.line_offsets.push(i + 1);
                line_count += 1;
                if line_count >= Self::MAX_LINES {
                    self.truncated = true;
                    break;
                }
            }
        }

        // If we didn't reach MAX_LINES, we're not truncated
        if line_count < Self::MAX_LINES {
            self.truncated = false;
        }

        self.scroll_line = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn total_lines(&self) -> usize {
        self.line_offsets.len()
    }

    pub fn current_scroll_line(&self) -> usize {
        self.scroll_line
    }

    /// Check if content was truncated at MAX_LINES
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Return full text (for copy operations)
    pub fn full_text(&self) -> &str {
        &self.text
    }

    /// Return a window of at most `max_lines` lines as a String
    pub fn window(&self, max_lines: usize) -> String {
        if self.text.is_empty() || max_lines == 0 {
            return String::new();
        }
        let total_lines = self.line_offsets.len();
        let start_line = self.scroll_line.min(total_lines.saturating_sub(1));
        let end_line = (start_line + max_lines).min(total_lines);

        let start_idx = self.line_offsets[start_line];
        let end_idx = if end_line < total_lines {
            self.line_offsets[end_line]
        } else {
            self.text.len()
        };
        self.text[start_idx..end_idx].to_string()
    }

    /// Scroll by delta lines (positive = down, negative = up)
    pub fn scroll_lines(&mut self, delta: isize, viewport_lines: usize) {
        if self.text.is_empty() {
            return;
        }
        let total_lines = self.line_offsets.len();
        let cur = self.scroll_line as isize;

        // Calculate the maximum scroll position based on viewport
        let max_scroll = if total_lines > viewport_lines {
            (total_lines - viewport_lines) as isize
        } else {
            0
        };

        // Clamp the next position to valid range
        let next = (cur + delta).max(0).min(max_scroll);
        self.scroll_line = next as usize;
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_line = 0;
    }

    pub fn scroll_to_bottom(&mut self, viewport_lines: usize) {
        let total = self.line_offsets.len();
        if total > viewport_lines {
            self.scroll_line = total - viewport_lines;
        } else {
            self.scroll_line = 0;
        }
    }
}

pub struct App {
    quit: bool,
    pane: usize, // 0 blocks, 1 txs, 2 details
    blocks: Vec<BlockRow>,
    sel_block_height: Option<u64>, // None = auto-follow newest, Some(height) = locked to specific block
    sel_tx: usize,

    // Details pane windowed rendering (virtual buffer)
    details_buf: DetailsBuffer,
    details_viewport_lines: usize, // Set by renderer based on pane height

    fps: u32,
    fps_choices: Vec<u32>,

    keep_blocks: usize,
    follow_blocks_latest: bool, // True = auto-follow newest block, False = locked to selection

    // Filter state
    filter_query: String,
    filter_compiled: CompiledFilter,
    input_mode: InputMode,

    // Search state
    search_query: String,
    search_results: Vec<crate::history::HistoryHit>,
    search_selection: usize,

    // Marks state
    marks_list: Vec<crate::types::Mark>,
    marks_selection: usize,

    // Manually-selected blocks cache (preserves blocks after they age out of rolling buffer)
    cached_blocks: HashMap<u64, BlockRow>, // height -> block
    cached_block_order: Vec<u64>,          // LRU tracking for cache eviction

    // Archival fetch state (for fetching historical blocks beyond cache)
    loading_block: Option<u64>, // Block height currently being fetched from archival
    archival_fetch_tx: Option<tokio::sync::mpsc::UnboundedSender<u64>>, // Channel to request archival fetches

    // FastNEAR API configuration (reserved for future FastNEAR /v0/transactions endpoint)
    #[allow(dead_code)]
    fastnear_api_url: String,
    #[allow(dead_code)]
    fastnear_auth_token: Option<String>,
    // Channel to request tx details: (tx_hash, sender_account_id)
    tx_details_fetch_tx: Option<tokio::sync::mpsc::UnboundedSender<(String, String)>>,

    // Performance: Cache parsed JSON to avoid expensive recursive parsing
    parsed_json_cache: HashMap<String, String>, // cache_key -> parsed_json (for blocks)

    // Transaction details cache (separate from blocks, bounded at MAX_FULL_TX)
    full_tx_cache: HashMap<String, String>, // tx_hash -> full archival JSON
    full_tx_order: Vec<String>,             // LRU tracking for tx cache eviction
    tx_details_in_flight: std::collections::HashSet<String>, // Dedupe in-flight requests

    /// When true, new live blocks from RPC are ignored.
    /// Set when user is pinned far behind the live tip (>50 blocks past focal).
    live_updates_paused: bool,

    // Backwards fill window (second list, anchored at selected block).
    back_slots: Vec<BackSlot>,
    back_anchor_height: Option<u64>,
    back_next_request_at: Option<Instant>,
    back_slots_target: usize,

    // Debug log (for development)
    debug_log: Vec<String>, // Rolling buffer of debug messages
    debug_visible: bool,    // Toggle debug panel visibility (Ctrl+D)

    // Keyboard shortcuts overlay (Web/Tauri only for now, TUI infrastructure ready for future)
    shortcuts_visible: bool, // Toggle keyboard shortcuts help overlay (? key)

    // Toast notification state
    toast_message: Option<(String, Instant)>, // (message, timestamp)

    // UI layout state
    details_fullscreen: bool, // Spacebar toggle for 100% details view
    fullscreen_content_type: FullscreenContentType, // What to show in fullscreen
    fullscreen_mode: FullscreenMode, // Scroll (arrow keys scroll JSON) or Navigate (arrow keys move rows)
    details_viewport_height: u16,    // Actual visible height of details pane (set by UI layer)

    // Theme (single source of truth for all UI targets)
    theme: Theme,

    // Cached ratatui styles (invalidated when theme changes)
    #[cfg(feature = "native")]
    rat_styles_cache: Option<ratatui_helpers::Styles>,

    // UI feature flags (for Web/Tauri enhanced behaviors)
    ui_flags: UiFlags,
}

impl App {
    pub fn new(
        fps: u32,
        fps_choices: Vec<u32>,
        keep_blocks: usize,
        default_filter: String,
        archival_fetch_tx: Option<tokio::sync::mpsc::UnboundedSender<u64>>,
        fastnear_api_url: String,
        fastnear_auth_token: Option<String>,
        tx_details_fetch_tx: Option<tokio::sync::mpsc::UnboundedSender<(String, String)>>,
    ) -> Self {
        let filter_compiled = if default_filter.is_empty() {
            CompiledFilter::default()
        } else {
            compile_filter(&default_filter)
        };

        Self {
            quit: false,
            pane: 0,
            blocks: Vec::with_capacity(keep_blocks),
            sel_block_height: None,
            sel_tx: 0, // Start in auto-follow mode
            details_buf: {
                let mut buf = DetailsBuffer::new();
                buf.set_text("(No blocks yet)".into());
                buf
            },
            details_viewport_lines: 32, // Sensible default, updated by renderer
            fps,
            fps_choices,
            keep_blocks,
            follow_blocks_latest: true, // Start in auto-follow mode
            filter_query: default_filter,
            filter_compiled,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selection: 0,
            marks_list: Vec::new(),
            marks_selection: 0,
            cached_blocks: HashMap::new(),
            cached_block_order: Vec::new(),
            loading_block: None,
            archival_fetch_tx,
            fastnear_api_url,
            fastnear_auth_token,
            tx_details_fetch_tx,
            parsed_json_cache: HashMap::new(),
            full_tx_cache: HashMap::new(),
            full_tx_order: Vec::new(),
            tx_details_in_flight: std::collections::HashSet::new(),
            live_updates_paused: false, // Start with live updates enabled
            back_slots: Vec::new(),
            back_anchor_height: None,
            back_next_request_at: None,
            back_slots_target: BACK_WINDOW,
            debug_log: Vec::new(),
            debug_visible: false,     // Hidden by default
            shortcuts_visible: false, // Hidden by default (Web/Tauri only for now)
            toast_message: None,
            details_fullscreen: false, // Normal view by default
            fullscreen_content_type: FullscreenContentType::ParsedDetails, // Default to parsed view
            fullscreen_mode: FullscreenMode::Scroll, // Scroll mode by default
            details_viewport_height: 20, // Default estimate, will be updated by UI
            theme: Theme::default(),   // Single source of truth for UI colors
            #[cfg(feature = "native")]
            rat_styles_cache: None, // Computed on first use
            ui_flags: UiFlags::default(), // Safe defaults for Web/Tauri
        }
    }

    // ----- getters -----
    pub fn fps(&self) -> u32 {
        self.fps
    }
    pub fn quit_flag(&self) -> bool {
        self.quit
    }
    pub fn pane(&self) -> usize {
        self.pane
    }
    pub fn sel_tx(&self) -> usize {
        self.sel_tx
    }
    pub fn is_viewing_cached_block(&self) -> bool {
        if let Some(height) = self.sel_block_height {
            self.find_block_index(Some(height)).is_none()
                && self.cached_blocks.contains_key(&height)
        } else {
            false
        }
    }

    /// Find the array index for a given block height
    /// Returns None if height is None (auto-follow mode) or block not found
    fn find_block_index(&self, height: Option<u64>) -> Option<usize> {
        if self.blocks.is_empty() {
            return None;
        }

        match height {
            None => None, // Auto-follow mode: return None to trigger auto-follow logic in current_block()
            Some(h) => self.blocks.iter().position(|b| b.height == h),
        }
    }

    /// Get the currently selected block (fallback to cache if aged out of main buffer)
    /// Filter-aware in auto-follow mode: returns first block with matching transactions
    pub fn current_block(&self) -> Option<&BlockRow> {
        if let Some(idx) = self.find_block_index(self.sel_block_height) {
            // Manual mode: return block at specific height
            self.blocks.get(idx)
        } else if let Some(height) = self.sel_block_height {
            // Block not in main buffer, check cache
            self.cached_blocks.get(&height)
        } else {
            // Auto-follow mode: respect filter
            if filter::is_empty(&self.filter_compiled) {
                // No filter: return newest block
                self.blocks.first()
            } else {
                // Filter active: return first block with matching transactions
                self.blocks.iter().find(|b| self.count_matching_txs(b) > 0)
            }
        }
    }

    fn block_by_height(&self, height: u64) -> Option<&BlockRow> {
        self.blocks
            .iter()
            .find(|b| b.height == height)
            .or_else(|| self.cached_blocks.get(&height))
    }

    pub fn back_slots(&self) -> &[BackSlot] {
        &self.back_slots
    }

    pub fn loading_block(&self) -> Option<u64> {
        self.loading_block
    }

    /// Count how many transactions in a block match the current filter
    fn count_matching_txs(&self, block: &BlockRow) -> usize {
        if filter::is_empty(&self.filter_compiled) {
            return block.transactions.len(); // No filter = all match
        }

        block
            .transactions
            .iter()
            .filter(|tx| {
                // Apply text filter
                let v = json!({
                    "hash": &tx.hash,
                    "signer_id": tx.signer_id.as_deref().unwrap_or(""),
                    "receiver_id": tx.receiver_id.as_deref().unwrap_or("")
                });
                tx_matches_filter(&v, &self.filter_compiled)
            })
            .count()
    }

    /// Returns blocks that have at least one matching transaction
    /// Returns (filtered_blocks, selected_index, total_count)
    pub fn filtered_blocks(&self) -> (Vec<&BlockRow>, Option<usize>, usize) {
        let total = self.blocks.len();

        // Check if we're viewing a cached block (not in main buffer)
        let viewing_cached = if let Some(height) = self.sel_block_height {
            self.find_block_index(Some(height)).is_none()
                && self.cached_blocks.contains_key(&height)
        } else {
            false
        };

        if filter::is_empty(&self.filter_compiled) {
            // No filter active
            let mut all_blocks: Vec<&BlockRow> = self.blocks.iter().collect();

            // If viewing cached block, inject it at correct position
            if viewing_cached {
                if let Some(cached_block) = self
                    .sel_block_height
                    .and_then(|h| self.cached_blocks.get(&h))
                {
                    // Find insertion point (sorted by height descending)
                    let insert_pos = all_blocks
                        .iter()
                        .position(|b| b.height < cached_block.height)
                        .unwrap_or(all_blocks.len());

                    all_blocks.insert(insert_pos, cached_block);
                }
            }

            // Find selection index in the (possibly injected) list
            let idx = self
                .sel_block_height
                .and_then(|h| all_blocks.iter().position(|b| b.height == h))
                .or(if !all_blocks.is_empty() {
                    Some(0)
                } else {
                    None
                });

            return (all_blocks, idx, total);
        }

        // Filter active: only show blocks with matching transactions
        let mut filtered: Vec<&BlockRow> = self
            .blocks
            .iter()
            .filter(|block| self.count_matching_txs(block) > 0)
            .collect();

        // If viewing cached block AND it has matching txs, inject it
        if viewing_cached {
            if let Some(cached_block) = self
                .sel_block_height
                .and_then(|h| self.cached_blocks.get(&h))
            {
                if self.count_matching_txs(cached_block) > 0 {
                    let insert_pos = filtered
                        .iter()
                        .position(|b| b.height < cached_block.height)
                        .unwrap_or(filtered.len());

                    filtered.insert(insert_pos, cached_block);
                }
            }
        }

        // Find selected block index in filtered list
        let sel_idx = if let Some(height) = self.sel_block_height {
            filtered
                .iter()
                .position(|b| b.height == height)
                .or(if !filtered.is_empty() { Some(0) } else { None })
        } else if !filtered.is_empty() {
            Some(0) // Auto-follow: select first filtered block (newest with matches)
        } else {
            None // No matching blocks
        };

        (filtered, sel_idx, total)
    }

    /// Get the list of blocks to navigate through (respects current filter)
    /// Returns Vec of heights in display order (newest first)
    fn get_navigation_list(&self) -> Vec<u64> {
        // Build list from main buffer (filtered or not)
        let mut nav_list: Vec<u64> = if filter::is_empty(&self.filter_compiled) {
            self.blocks.iter().map(|b| b.height).collect()
        } else {
            self.blocks
                .iter()
                .filter(|b| self.count_matching_txs(b) > 0)
                .map(|b| b.height)
                .collect()
        };

        // If viewing cached block, inject it into nav list at correct position
        if let Some(height) = self.sel_block_height {
            if self.find_block_index(Some(height)).is_none()
                && self.cached_blocks.contains_key(&height)
            {
                // Cached block - insert into nav list
                let insert_pos = nav_list
                    .iter()
                    .position(|&h| h < height)
                    .unwrap_or(nav_list.len());

                nav_list.insert(insert_pos, height);
            }
        }

        nav_list
    }

    /// Check if a specific block height is available (in buffer or cache)
    pub fn is_block_height_available(&self, height: u64) -> bool {
        self.is_block_available(height)
    }

    /// Get the currently selected block's height (for UI purposes)
    pub fn selected_block_height(&self) -> Option<u64> {
        self.sel_block_height.or_else(|| {
            // Auto-follow mode: return newest block height if available
            self.blocks.first().map(|b| b.height)
        })
    }

    pub fn txs(&self) -> (Vec<TxLite>, usize, usize) {
        if let Some(b) = self.current_block() {
            let total = b.transactions.len();
            let filtered: Vec<TxLite> = b
                .transactions
                .iter()
                .filter(|tx| {
                    // Apply text filter - pass complete tx data for filtering
                    // Note: actions omitted here since ActionSummary doesn't derive Serialize
                    // TODO: Add Serialize to ActionSummary for full filtering support
                    let v = json!({
                        "hash": &tx.hash,
                        "signer_id": tx.signer_id.as_deref().unwrap_or(""),
                        "receiver_id": tx.receiver_id.as_deref().unwrap_or("")
                    });
                    tx_matches_filter(&v, &self.filter_compiled)
                })
                .cloned()
                .collect();
            (filtered, self.sel_tx, total)
        } else {
            (vec![], 0, 0)
        }
    }

    pub fn details(&self) -> &str {
        self.details_buf.full_text()
    }
    pub fn details_scroll(&self) -> u16 {
        self.details_buf.current_scroll_line() as u16
    }
    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }
    pub fn filter_query(&self) -> &str {
        &self.filter_query
    }
    pub fn debug_log(&self) -> &[String] {
        &self.debug_log
    }
    pub fn debug_visible(&self) -> bool {
        self.debug_visible
    }
    pub fn details_fullscreen(&self) -> bool {
        self.details_fullscreen
    }
    pub fn fullscreen_content_type(&self) -> FullscreenContentType {
        self.fullscreen_content_type
    }
    pub fn fullscreen_mode(&self) -> FullscreenMode {
        self.fullscreen_mode
    }

    /// Get text for selection slot (shows current block context)
    pub fn selection_slot_text(&self) -> String {
        // Show currently selected block
        if let Some(block) = self.current_block() {
            // Check if THIS block is currently loading
            let is_loading = self.loading_block == Some(block.height);
            if self.follow_blocks_latest && self.sel_block_height.is_none() {
                // Auto-follow mode: show filtered latest block
                format!("► Auto-follow: Block #{} (latest)", block.height)
            } else if is_loading {
                // This specific block is being fetched from archival
                format!("► Selected: Block #{} (loading...)", block.height)
            } else {
                // Manual selection mode: show block details with timestamp
                format!(
                    "► Selected: Block #{} ({} txs) · {}",
                    block.height, block.tx_count, block.when
                )
            }
        } else {
            // No blocks available
            if self.follow_blocks_latest && self.sel_block_height.is_none() {
                "► Auto-follow (waiting for blocks...)".to_string()
            } else {
                "► No block selected (waiting for blocks...)".to_string()
            }
        }
    }

    /// Get the active theme (single source of truth for UI colors)
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Get raw JSON of currently selected block (for fullscreen display/copying)
    pub fn get_raw_block_json(&mut self) -> String {
        // Check if we have any blocks loaded at all
        if self.blocks.is_empty() && self.cached_blocks.is_empty() {
            return "Waiting for blocks to load...".to_string();
        }

        match self.current_block() {
            Some(block) => {
                let cache_key = format!("block_{}", block.height);

                // Check cache first to avoid expensive recursive parsing
                if let Some(cached) = self.parsed_json_cache.get(&cache_key) {
                    return cached.clone();
                }

                // Cache miss - compute and store
                match serde_json::to_value(block) {
                    Ok(mut val) => {
                        // Guard against null values (shouldn't happen but was in old code)
                        if val.is_null() {
                            "Error: Block serialized to null".to_string()
                        } else {
                            // Apply recursive JSON parsing to nested string fields
                            val = crate::json_auto_parse::auto_parse_nested_json(val, 5, 0);
                            let raw_json = crate::json_pretty::pretty_safe(&val, 2, 100 * 1024);

                            self.parsed_json_cache.insert(cache_key, raw_json.clone());

                            // Evict cache if too large
                            if self.parsed_json_cache.len() > 100 {
                                self.parsed_json_cache.clear();
                            }

                            raw_json
                        }
                    }
                    Err(e) => {
                        format!("Error: Failed to serialize block - {}", e)
                    }
                }
            }
            None => {
                // Provide more context about why no block is selected
                if let Some(height) = self.sel_block_height {
                    format!("Block {} not found in buffer", height)
                } else {
                    "No block selected (auto-follow mode)".to_string()
                }
            }
        }
    }

    /// Get raw JSON of currently selected transaction (for fullscreen display/copying)
    pub fn get_raw_tx_json(&mut self) -> String {
        if let Some(block) = self.current_block() {
            if let Some(tx) = block.transactions.get(self.sel_tx) {
                // Check full_tx_cache first for archival RPC data
                if let Some(full_json) = self.full_tx_cache.get(&tx.hash) {
                    return full_json.clone();
                }

                // Fall back to parsed_json_cache (for TxLite from chunks)
                let cache_key = format!("tx_{}", tx.hash);
                if let Some(cached) = self.parsed_json_cache.get(&cache_key) {
                    return cached.clone();
                }

                // Cache miss - compute TxLite JSON and store
                let mut val = serde_json::to_value(tx).unwrap_or(serde_json::Value::Null);
                val = crate::json_auto_parse::auto_parse_nested_json(val, 5, 0);
                let raw_json = crate::json_pretty::pretty_safe(&val, 2, 100 * 1024);

                self.parsed_json_cache.insert(cache_key, raw_json.clone());

                // Evict cache if too large
                if self.parsed_json_cache.len() > 100 {
                    self.parsed_json_cache.clear();
                }

                return raw_json;
            }
        }
        "No transaction selected".to_string()
    }

    /// Set the active theme (for runtime theme switching)
    pub fn set_theme(&mut self, theme: Theme) {
        if self.theme != theme {
            self.theme = theme;
            #[cfg(feature = "native")]
            {
                self.rat_styles_cache = None; // Invalidate cached styles
            }
        }
    }

    /// Get cached ratatui styles for current theme (computed on first use, invalidated on theme change)
    #[cfg(feature = "native")]
    pub fn rat_styles(&mut self) -> ratatui_helpers::Styles {
        if let Some(ref styles) = self.rat_styles_cache {
            return *styles;
        }
        let styles = ratatui_helpers::styles(&self.theme);
        self.rat_styles_cache = Some(styles);
        styles
    }

    /// Get UI feature flags (controls Web/Tauri enhanced behaviors)
    pub fn ui_flags(&self) -> UiFlags {
        self.ui_flags
    }

    /// Set UI feature flags (for runtime toggling or testing)
    pub fn set_ui_flags(&mut self, flags: UiFlags) {
        self.ui_flags = flags;
    }

    /// Set the actual viewport height of details pane (called from UI layer)
    pub fn set_details_viewport_height(&mut self, height: u16) {
        self.details_viewport_height = height;
    }

    /// Show a toast notification for 2 seconds
    pub fn show_toast(&mut self, msg: String) {
        self.toast_message = Some((msg, Instant::now()));
    }

    /// Get current toast message if still active (visible for 2 seconds)
    pub fn toast_message(&self) -> Option<&str> {
        const TOAST_DURATION: Duration = Duration::from_secs(2);
        self.toast_message.as_ref().and_then(|(msg, time)| {
            if time.elapsed() < TOAST_DURATION {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    // ----- knobs -----
    pub fn cycle_fps(&mut self) {
        if self.fps_choices.is_empty() {
            return;
        }
        let mut idx = self
            .fps_choices
            .iter()
            .position(|&v| v == self.fps)
            .unwrap_or(0);
        idx = (idx + 1) % self.fps_choices.len();
        self.fps = self.fps_choices[idx];
    }

    pub fn log_debug(&mut self, msg: String) {
        const MAX_LOG_ENTRIES: usize = 50;

        // Write to file for debugging (native only - WASM doesn't have filesystem)
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            let timestamp = chrono::Utc::now().format("%H:%M:%S%.3f");
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("nearx_debug.log")
            {
                let _ = writeln!(file, "[{timestamp}] {msg}");
            }
        }

        // Also keep in memory for debug panel
        self.debug_log.push(msg);
        if self.debug_log.len() > MAX_LOG_ENTRIES {
            self.debug_log.remove(0);
        }
    }

    // ----- block cache methods -----
    /// Cache selected block and ±50 blocks around it for context navigation
    fn cache_block_with_context(&mut self, center_height: u64) {
        use crate::constants::app::CACHE_CONTEXT_BLOCKS;
        const MAX_TOTAL_CACHED: usize = 300; // Safety limit (3× context window)

        // Find the center block's index
        let center_idx = match self.find_block_index(Some(center_height)) {
            Some(idx) => idx,
            None => return, // Center block not in buffer, can't cache context
        };

        // Cache blocks in range [center - 12, center + 12]
        let start_idx = center_idx.saturating_sub(CACHE_CONTEXT_BLOCKS);
        let end_idx = (center_idx + CACHE_CONTEXT_BLOCKS + 1).min(self.blocks.len());

        let mut cached_count = 0;
        for idx in start_idx..end_idx {
            if let Some(block) = self.blocks.get(idx) {
                let height = block.height;

                // Update LRU: remove if exists, add to end
                self.cached_block_order.retain(|&h| h != height);
                self.cached_block_order.push(height);

                // Add to cache
                if let std::collections::hash_map::Entry::Vacant(e) =
                    self.cached_blocks.entry(height)
                {
                    e.insert(block.clone());
                    cached_count += 1;
                }
            }
        }

        // Evict oldest if over limit
        while self.cached_block_order.len() > MAX_TOTAL_CACHED {
            if let Some(old_height) = self.cached_block_order.first().copied() {
                self.cached_block_order.remove(0);
                self.cached_blocks.remove(&old_height);
            }
        }

        if cached_count > 0 {
            self.log_debug(format!(
                "Cached block #{} with ±{} context ({} new, {} total)",
                center_height,
                CACHE_CONTEXT_BLOCKS,
                cached_count,
                self.cached_blocks.len()
            ));
        }
    }

    /// Check if a block is available for viewing (in main buffer or cache)
    pub fn is_block_available(&self, height: u64) -> bool {
        self.find_block_index(Some(height)).is_some() || self.cached_blocks.contains_key(&height)
    }

    /// Eagerly fill ±50 block window around selected height via archival RPC
    ///
    /// For each height in [center-50, center+50]:
    /// - If block is available (in buffer or cache): skip
    /// - If block is missing: request from archival RPC
    ///
    /// This enables smooth navigation through historical blocks without gaps.
    pub fn ensure_block_window(&mut self, center_height: u64) {
        use crate::constants::app::ARCHIVAL_CONTEXT_BLOCKS;

        let start = center_height.saturating_sub(ARCHIVAL_CONTEXT_BLOCKS);
        let end = center_height + ARCHIVAL_CONTEXT_BLOCKS;

        let mut requested_count = 0;
        for h in start..=end {
            if !self.is_block_available(h) {
                self.request_archival_block(h);
                requested_count += 1;
            }
        }

        // Always cache what we already have
        self.cache_block_with_context(center_height);

        if requested_count > 0 {
            self.log_debug(format!(
                "Requested {} missing blocks in window [{}..={}] around #{}",
                requested_count, start, end, center_height
            ));
        }
    }

    /// Eagerly fill ±50 block window using canonical chain-walking on every selection change.
    ///
    /// - Walks backward using prev_hash (canonical chain)
    /// - Walks forward using height (no next_hash in protocol)
    /// - Respects latest known block boundary (can't fetch future)
    /// - Uses archival RPC for historical blocks
    pub fn ensure_block_window_by_chain(&mut self, center_height: u64) {
        use crate::constants::app::ARCHIVAL_CONTEXT_BLOCKS;

        // Determine latest known block height (can't request future blocks)
        let latest_known = self
            .blocks
            .first()
            .map(|b| b.height)
            .unwrap_or(center_height);

        // --- Walk BACKWARD (±50 blocks behind center) ---
        let backward_target = center_height.saturating_sub(ARCHIVAL_CONTEXT_BLOCKS);
        let mut backward_requested = 0;

        for h in backward_target..center_height {
            if !self.is_block_available(h) {
                self.request_archival_block(h);
                backward_requested += 1;
            }
        }

        // --- Walk FORWARD (±50 blocks ahead, capped at latest_known) ---
        let forward_target = (center_height + ARCHIVAL_CONTEXT_BLOCKS).min(latest_known);
        let mut forward_requested = 0;

        for h in (center_height + 1)..=forward_target {
            if !self.is_block_available(h) {
                self.request_archival_block(h);
                forward_requested += 1;
            }
        }

        if backward_requested > 0 || forward_requested > 0 {
            self.log_debug(format!(
                "[CHAIN-WALK] Block #{}: requested {} backward, {} forward (latest: {})",
                center_height, backward_requested, forward_requested, latest_known
            ));
        }

        // Cache what we already have
        self.cache_block_with_context(center_height);
    }

    /// Toggle debug panel visibility (Ctrl+D)
    pub fn toggle_debug_panel(&mut self) {
        self.debug_visible = !self.debug_visible;
        self.log_debug(format!(
            "Debug panel: {}",
            if self.debug_visible {
                "visible"
            } else {
                "hidden"
            }
        ));
    }

    /// Get keyboard shortcuts overlay visibility state
    pub fn show_shortcuts(&self) -> bool {
        self.shortcuts_visible
    }

    /// Toggle keyboard shortcuts overlay (? key - Web/Tauri only for now)
    pub fn toggle_shortcuts(&mut self) {
        self.shortcuts_visible = !self.shortcuts_visible;
        self.log_debug(format!(
            "Shortcuts overlay: {}",
            if self.shortcuts_visible {
                "visible"
            } else {
                "hidden"
            }
        ));
    }

    /// Hide keyboard shortcuts overlay (Esc key)
    pub fn hide_shortcuts(&mut self) {
        self.shortcuts_visible = false;
        self.log_debug("Shortcuts overlay: hidden".to_string());
    }

    /// Toggle details fullscreen mode (Spacebar - pane-aware)
    pub fn toggle_details_fullscreen(&mut self) {
        if self.details_fullscreen {
            // Exit fullscreen - always return to parsed details view and reset to Scroll mode
            self.details_fullscreen = false;
            self.fullscreen_content_type = FullscreenContentType::ParsedDetails;
            self.fullscreen_mode = FullscreenMode::Scroll;
            self.log_debug("Exited fullscreen, back to parsed details".to_string());

            // Restore the appropriate formatted view based on current selection
            if self.pane == 1 && self.current_block().is_some() {
                // Re-select current transaction to show formatted view
                self.select_tx();
            }
        } else {
            // Enter fullscreen - content depends on which pane is focused, start in Scroll mode
            self.details_fullscreen = true;
            self.fullscreen_mode = FullscreenMode::Scroll;
            self.fullscreen_content_type = match self.pane {
                0 => FullscreenContentType::BlockRawJson, // Blocks pane
                1 => FullscreenContentType::TransactionRawJson, // Txs pane
                2 => FullscreenContentType::ParsedDetails, // Details pane
                _ => FullscreenContentType::ParsedDetails, // Fallback
            };
            let content_type = match self.fullscreen_content_type {
                FullscreenContentType::BlockRawJson => "block raw JSON",
                FullscreenContentType::TransactionRawJson => "transaction raw JSON",
                FullscreenContentType::ParsedDetails => "parsed details",
            };
            self.log_debug(format!("Entered fullscreen showing: {content_type}"));

            // Compute and cache the JSON content when entering fullscreen
            match self.fullscreen_content_type {
                FullscreenContentType::BlockRawJson => {
                    let raw = self.get_raw_block_json();
                    self.set_details_json(raw);

                    // Eagerly fill ±50 block window
                    if let Some(block) = self.current_block() {
                        self.ensure_block_window(block.height);
                    }
                }
                FullscreenContentType::TransactionRawJson => {
                    let raw = self.get_raw_tx_json();
                    self.set_details_json(raw);
                }
                FullscreenContentType::ParsedDetails => {
                    // Already in buffer, no-op
                }
            }
        }
    }

    /// Toggle between Scroll and Navigate modes in fullscreen (Tab key)
    pub fn toggle_fullscreen_mode(&mut self) {
        self.fullscreen_mode = match self.fullscreen_mode {
            FullscreenMode::Scroll => FullscreenMode::Navigate,
            FullscreenMode::Navigate => FullscreenMode::Scroll,
        };
        self.log_debug(format!("Fullscreen mode: {:?}", self.fullscreen_mode));
    }

    /// Return to auto-follow mode (track newest block)
    pub fn return_to_auto_follow(&mut self) {
        let old_height = self.sel_block_height;
        self.follow_blocks_latest = true; // Re-enable auto-follow mode
        self.sel_block_height = None; // None = auto-follow newest
        self.sel_tx = 0; // Reset to first tx
        if !self.blocks.is_empty() {
            self.validate_and_refresh_tx(BlockChangeReason::AutoFollow);
            self.log_debug(format!("[USER_ACTION] Return to AUTO-FOLLOW mode (Home key pressed), was locked to height={old_height:?}"));
        }
    }

    // ----- filter methods -----
    pub fn start_filter(&mut self) {
        self.input_mode = InputMode::Filter;
    }

    pub fn clear_filter(&mut self) {
        self.filter_query.clear();
        self.filter_compiled = CompiledFilter::default();
        self.input_mode = InputMode::Normal;
        self.validate_and_refresh_tx(BlockChangeReason::FilterChange); // Try to preserve tx
    }

    pub fn apply_filter(&mut self) {
        self.filter_compiled = compile_filter(&self.filter_query);
        self.input_mode = InputMode::Normal;
        self.validate_and_refresh_tx(BlockChangeReason::FilterChange); // Try to preserve tx
    }

    pub fn filter_add_char(&mut self, ch: char) {
        self.filter_query.push(ch);
    }

    pub fn filter_backspace(&mut self) {
        self.filter_query.pop();
    }

    // ----- copy functionality -----
    /// Get copy content for the currently focused pane.
    ///
    /// Delegates to `copy_api` for consistent behavior across all targets.
    pub fn get_copy_content(&mut self) -> String {
        crate::copy_api::current_text(self).unwrap_or_default()
    }

    // ----- selection / scrolling -----
    /// Move focus to next pane (circular: 0→1→2→0)
    pub fn next_pane(&mut self) {
        self.pane = (self.pane + 1) % 3;
        self.log_debug(format!("Tab -> pane={}", self.pane));
    }

    /// Move focus to previous pane (circular: 2→1→0→2)
    pub fn prev_pane(&mut self) {
        // Backward navigation: subtract 1 with wrap-around
        // (pane - 1 + 3) % 3 ensures we don't underflow (e.g., 0-1 = -1)
        self.pane = (self.pane + 3 - 1) % 3;
        self.log_debug(format!("BackTab -> pane={}", self.pane));
    }

    /// Set pane directly (used by deep link router)
    pub fn set_pane_direct(&mut self, pane: usize) {
        if pane < 3 {
            self.pane = pane;
            self.log_debug(format!("DeepLink -> pane={}", self.pane));
        }
    }

    fn lock_to_block_height_from_route(&mut self, height: u64) {
        self.sel_block_height = Some(height);
        self.follow_blocks_latest = false;

        if self.is_block_available(height) {
            self.cache_block_with_context(height);
        } else {
            self.request_archival_block(height);
        }

        self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
    }

    fn lock_to_block_hash_from_route(&mut self, block_hash: &str) -> bool {
        let height = self
            .blocks
            .iter()
            .find(|b| b.hash.eq_ignore_ascii_case(block_hash))
            .map(|b| b.height)
            .or_else(|| {
                self.cached_blocks
                    .iter()
                    .find(|(_, b)| b.hash.eq_ignore_ascii_case(block_hash))
                    .map(|(h, _)| *h)
            });

        if let Some(h) = height {
            self.lock_to_block_height_from_route(h);
            true
        } else {
            false
        }
    }

    /// Apply a deep link route to the current app state
    ///
    /// This method maps deep link routes to the existing explorer UI by:
    /// - Setting the appropriate pane focus
    /// - Applying filters to match the route target
    ///
    /// Example routes:
    /// - `Tx` → Focus transactions pane, filter to transaction hash
    /// - `Block` → Focus blocks pane and lock to specific block
    /// - `Account` / `Contract` / `AccessKey` → Apply scoped filters
    pub fn apply_route(&mut self, route: &crate::router::Route) {
        use crate::router::{BlockRef, Route, RouteV1};

        match route {
            Route::V1(RouteV1::Tx {
                tx_hash,
                block,
                network,
            }) => {
                self.set_pane_direct(1);
                self.filter_query = format!("hash:{tx_hash}");
                self.apply_filter();

                if let Some(block_hint) = block {
                    match block_hint {
                        BlockRef::Height(height) => self.lock_to_block_height_from_route(*height),
                        BlockRef::Hash(hash) => {
                            if !self.lock_to_block_hash_from_route(hash) {
                                self.show_toast(format!(
                                    "Block hash hint not found in local buffer: {hash}"
                                ));
                            }
                        }
                    }
                }

                self.log_debug(format!(
                    "Route: tx/{tx_hash} block_hint={block:?} network={network:?}"
                ));
            }
            Route::V1(RouteV1::Block { block, network }) => {
                self.set_pane_direct(0);
                self.clear_filter();

                match block {
                    BlockRef::Height(height) => self.lock_to_block_height_from_route(*height),
                    BlockRef::Hash(hash) => {
                        if !self.lock_to_block_hash_from_route(hash) {
                            self.show_toast(format!(
                                "Block hash not in local buffer; try block height: {hash}"
                            ));
                        }
                    }
                }

                self.log_debug(format!("Route: block/{block:?} network={network:?}"));
            }
            Route::V1(RouteV1::Account {
                account_id,
                network,
            }) => {
                self.set_pane_direct(1);
                self.filter_query = format!("acct:{account_id}");
                self.apply_filter();
                self.log_debug(format!("Route: account/{account_id} network={network:?}"));
            }
            Route::V1(RouteV1::Contract {
                account_id,
                method,
                network,
            }) => {
                self.set_pane_direct(1);
                let mut query = format!("receiver:{account_id} action:FunctionCall,DeployContract");
                if let Some(m) = method {
                    query.push_str(" method:");
                    query.push_str(m);
                }
                self.filter_query = query;
                self.apply_filter();
                self.log_debug(format!(
                    "Route: contract/{account_id} method={method:?} network={network:?}"
                ));
            }
            Route::V1(RouteV1::AccessKey {
                account_id,
                public_key,
                network,
            }) => {
                self.set_pane_direct(1);
                self.filter_query =
                    format!("acct:{account_id} action:AddKey,DeleteKey raw:{public_key}");
                self.apply_filter();
                self.log_debug(format!(
                    "Route: access-key/{account_id}/{public_key} network={network:?}"
                ));
            }
            Route::V1(RouteV1::Home) => {
                // Clear filter and return to auto-follow mode
                self.clear_filter();
                self.return_to_auto_follow();
                self.log_debug("Route: home".to_string());
            }
            Route::V1(RouteV1::Staking {
                account_id,
                network,
            }) => {
                // Staking is web/Tauri-only; TUI ignores this route
                self.log_debug(format!(
                    "Route: staking account={account_id:?} network={network:?} (ignored in TUI)"
                ));
            }
        }
    }

    /// Refresh tx details if current selection is still valid, otherwise reset
    fn validate_and_refresh_tx(&mut self, reason: BlockChangeReason) {
        let (txs, _, _) = self.txs();

        match reason {
            BlockChangeReason::AutoFollow => {
                // Auto-follow: keep tx index if valid, clamp if out of bounds
                if self.sel_tx >= txs.len() {
                    self.sel_tx = txs.len().saturating_sub(1);
                }
                if !txs.is_empty() {
                    self.select_tx();
                }
            }
            BlockChangeReason::ManualNav => {
                // Manual navigation: reset to first tx in new block
                self.sel_tx = 0;
                if !txs.is_empty() {
                    self.select_tx();
                }
            }
            BlockChangeReason::FilterChange => {
                // Filter change: preserve tx if valid, otherwise reset
                if self.sel_tx >= txs.len() {
                    self.sel_tx = 0;
                }
                if !txs.is_empty() {
                    self.select_tx();
                }
            }
        }

        // If in fullscreen mode showing block JSON, update it when block changes
        if self.details_fullscreen
            && self.fullscreen_content_type == FullscreenContentType::BlockRawJson
        {
            let raw = self.get_raw_block_json();
            self.set_details_json(raw);
        }
    }

    /// Handle Up arrow key - behavior depends on which pane is focused
    pub fn up(&mut self) {
        // Fullscreen Navigate mode: route to appropriate pane based on content type
        if self.details_fullscreen && self.fullscreen_mode == FullscreenMode::Navigate {
            match self.fullscreen_content_type {
                FullscreenContentType::BlockRawJson => {
                    // Navigate blocks (temporarily switch pane logic)
                    let saved_pane = self.pane;
                    self.pane = 0;
                    self.up(); // Recursive call with pane 0
                    self.pane = saved_pane;
                    return;
                }
                FullscreenContentType::TransactionRawJson => {
                    // Navigate transactions
                    let saved_pane = self.pane;
                    self.pane = 1;
                    self.up(); // Recursive call with pane 1
                    self.pane = saved_pane;
                    return;
                }
                FullscreenContentType::ParsedDetails => {
                    // Parsed view has no selection, just scroll
                    self.scroll_details(-1);
                    return;
                }
            }
        }

        match self.pane {
            0 => {
                // Blocks pane: navigate to previous block (newer)
                self.log_debug(format!(
                    "[USER_NAV_UP] follow_latest={}, sel_height={:?}",
                    self.follow_blocks_latest, self.sel_block_height
                ));

                // Get the navigation list (respects filter)
                let nav_list = self.get_navigation_list();

                if nav_list.is_empty() {
                    return; // No blocks to navigate
                }

                // Ensure we have a locked selection (handle edge case of None)
                let current_height = match self.sel_block_height {
                    Some(h) => h,
                    None => {
                        // Edge case: not locked yet, lock to current and don't navigate
                        // This shouldn't happen with auto-lock, but handle gracefully
                        let h = nav_list[0];
                        self.sel_block_height = Some(h);
                        self.follow_blocks_latest = false; // User navigation disables auto-follow
                        self.cache_block_with_context(h);
                        self.log_debug(format!("[USER_NAV_UP] edge case lock to #{h}"));
                        return;
                    }
                };

                // Navigate to next newer block in navigation list
                if let Some(current_idx) = nav_list.iter().position(|&h| h == current_height) {
                    if current_idx > 0 {
                        let new_height = nav_list[current_idx - 1];
                        // Only navigate if target block is available
                        if self.is_block_available(new_height) {
                            self.sel_block_height = Some(new_height);
                            self.follow_blocks_latest = false; // User navigation disables auto-follow
                            self.cache_block_with_context(new_height);
                            self.ensure_block_window_by_chain(new_height); // Chain-walk backfill
                            self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
                            self.log_debug(format!("Blocks UP -> #{new_height}"));
                        } else {
                            // Block not available - try archival fetch
                            self.log_debug(format!("Blocks UP -> #{new_height} not available"));
                            self.request_archival_block(new_height);
                        }
                    }
                } else {
                    // Current selection not in navigation list (filtered out), jump to newest
                    let new_height = nav_list[0];
                    self.sel_block_height = Some(new_height);
                    self.follow_blocks_latest = false; // User navigation disables auto-follow
                    self.cache_block_with_context(new_height);
                    self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
                    self.log_debug(format!(
                        "Blocks UP -> not in list, jump to newest #{new_height}"
                    ));
                }
            }
            1 => {
                // Tx pane: navigate to previous transaction
                if self.sel_tx > 0 {
                    self.sel_tx -= 1;
                    self.select_tx();
                    self.log_debug(format!("Tx UP, sel={}", self.sel_tx));
                }
            }
            2 => {
                // Details pane: scroll up
                self.scroll_details(-1);
            }
            _ => {}
        }
    }
    /// Handle Down arrow key - behavior depends on which pane is focused
    pub fn down(&mut self) {
        // Fullscreen Navigate mode: route to appropriate pane based on content type
        if self.details_fullscreen && self.fullscreen_mode == FullscreenMode::Navigate {
            match self.fullscreen_content_type {
                FullscreenContentType::BlockRawJson => {
                    // Navigate blocks (temporarily switch pane logic)
                    let saved_pane = self.pane;
                    self.pane = 0;
                    self.down(); // Recursive call with pane 0
                    self.pane = saved_pane;
                    return;
                }
                FullscreenContentType::TransactionRawJson => {
                    // Navigate transactions
                    let saved_pane = self.pane;
                    self.pane = 1;
                    self.down(); // Recursive call with pane 1
                    self.pane = saved_pane;
                    return;
                }
                FullscreenContentType::ParsedDetails => {
                    // Parsed view has no selection, just scroll
                    self.scroll_details(1);
                    return;
                }
            }
        }

        match self.pane {
            0 => {
                // Blocks pane: navigate to next block (older)
                self.log_debug(format!(
                    "[USER_NAV_DOWN] follow_latest={}, sel_height={:?}",
                    self.follow_blocks_latest, self.sel_block_height
                ));

                // Get the navigation list (respects filter)
                let nav_list = self.get_navigation_list();

                if nav_list.is_empty() {
                    return; // No blocks to navigate
                }

                // Ensure we have a locked selection (handle edge case of None)
                let current_height = match self.sel_block_height {
                    Some(h) => h,
                    None => {
                        // Edge case: not locked yet, lock to current and navigate down
                        // With auto-lock, this means we want to move to NEXT block
                        let h = nav_list[0];
                        if nav_list.len() > 1 {
                            // Move to next older block
                            let next_h = nav_list[1];
                            self.sel_block_height = Some(next_h);
                            self.follow_blocks_latest = false; // User navigation disables auto-follow
                            self.cache_block_with_context(next_h);
                            self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
                            self.log_debug(format!(
                                "[USER_NAV_DOWN] first press, move to #{next_h}"
                            ));
                        } else {
                            // Only one block, just lock to it
                            self.sel_block_height = Some(h);
                            self.follow_blocks_latest = false; // User navigation disables auto-follow
                            self.cache_block_with_context(h);
                            self.log_debug(format!("Blocks DOWN -> only one block, lock to #{h}"));
                        }
                        return;
                    }
                };

                // Navigate to next older block in navigation list
                if let Some(current_idx) = nav_list.iter().position(|&h| h == current_height) {
                    if current_idx + 1 < nav_list.len() {
                        let new_height = nav_list[current_idx + 1];
                        // Only navigate if target block is available
                        if self.is_block_available(new_height) {
                            self.sel_block_height = Some(new_height);
                            self.follow_blocks_latest = false; // User navigation disables auto-follow
                            self.cache_block_with_context(new_height);
                            self.ensure_block_window_by_chain(new_height); // Chain-walk backfill
                            self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
                            self.log_debug(format!("Blocks DOWN -> #{new_height}"));
                        } else {
                            // Block not available - try archival fetch
                            self.log_debug(format!("Blocks DOWN -> #{new_height} not available"));
                            self.request_archival_block(new_height);
                        }
                    }
                } else {
                    // Current selection not in navigation list (filtered out), jump to newest
                    let new_height = nav_list[0];
                    self.sel_block_height = Some(new_height);
                    self.follow_blocks_latest = false; // User navigation disables auto-follow
                    self.cache_block_with_context(new_height);
                    self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
                    self.log_debug(format!(
                        "Blocks DOWN -> not in list, jump to newest #{new_height}"
                    ));
                }
            }
            1 => {
                // Tx pane: navigate to next transaction
                let (txs, _, _) = self.txs();
                if self.sel_tx + 1 < txs.len() {
                    self.sel_tx += 1;
                    self.select_tx();
                    self.log_debug(format!("Tx DOWN, sel={}", self.sel_tx));
                }
            }
            2 => {
                // Details pane: scroll down
                self.scroll_details(1);
            }
            _ => {}
        }
    }

    /// Select a block by row index (for mouse mapping)
    pub fn select_block_row(&mut self, idx: usize) {
        let nav_list = self.get_navigation_list();
        if let Some(&height) = nav_list.get(idx) {
            self.sel_block_height = Some(height);
            self.follow_blocks_latest = false; // User interaction disables auto-follow
            self.cache_block_with_context(height);
            self.ensure_block_window_by_chain(height); // Chain-walk backfill
            self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
            self.log_debug(format!("Mouse select block #{height} (idx {idx})"));
        }
    }

    /// Select a transaction by row index (for mouse mapping)
    pub fn select_tx_row(&mut self, idx: usize) {
        let (txs, _, _) = self.txs();
        if idx < txs.len() {
            self.sel_tx = idx;
            self.select_tx();
            self.log_debug(format!("Mouse select tx (idx {idx})"));
        }
    }

    /// Left arrow: Jump to top of current list
    pub fn left(&mut self) {
        match self.pane {
            0 => {
                // Blocks pane: "go to current" – jump to tip and resume live stream.
                if !self.blocks.is_empty() {
                    // Clear any manual anchor and resume following the live head.
                    self.sel_block_height = None;
                    self.follow_blocks_latest = true;
                    self.live_updates_paused = false;

                    // Reset backwards window so it re-anchors to the new selection.
                    self.back_slots.clear();
                    self.back_anchor_height = None;
                    self.back_next_request_at = None;

                    self.validate_and_refresh_tx(BlockChangeReason::AutoFollow);
                }
            }
            1 => {
                // Jump to first tx
                if self.sel_tx != 0 {
                    self.sel_tx = 0;
                    self.select_tx();
                    self.log_debug("Left -> jump to first tx".into());
                }
            }
            2 => {
                // Scroll to top of details
                if self.details_scroll() != 0 {
                    self.log_debug("Left -> scroll to top".into());
                }
                self.details_home();
            }
            _ => {}
        }
    }

    /// Right arrow: Paginate down 12 items
    pub fn right(&mut self) {
        match self.pane {
            0 => {
                // Paginate down 12 blocks (toward older) - uses height-based navigation
                let nav_list = self.get_navigation_list();

                if nav_list.is_empty() {
                    return;
                }

                // Get current height (or default to newest)
                let current_height = self.sel_block_height.unwrap_or_else(|| nav_list[0]);

                // Find position in navigation list
                if let Some(current_idx) = nav_list.iter().position(|&h| h == current_height) {
                    // Jump 12 positions in navigation list (respects filter + sorted order)
                    let new_idx = (current_idx + 12).min(nav_list.len() - 1);

                    if new_idx != current_idx {
                        let new_height = nav_list[new_idx];
                        self.sel_block_height = Some(new_height); // Lock to specific height
                        self.follow_blocks_latest = false; // User navigation disables auto-follow
                        self.ensure_block_window_by_chain(new_height); // Trigger archival backfill
                        self.validate_and_refresh_tx(BlockChangeReason::ManualNav);

                        self.log_debug(format!(
                            "[NAV_RIGHT] Paginated from #{} to #{} (+12 in nav list, idx {} -> {})",
                            current_height, new_height, current_idx, new_idx
                        ));
                    }
                }
            }
            1 => {
                // Paginate down 12 txs
                let (txs, _, _) = self.txs();
                let new_sel = (self.sel_tx + 12).min(txs.len().saturating_sub(1));
                if new_sel != self.sel_tx && !txs.is_empty() {
                    self.sel_tx = new_sel;
                    self.select_tx();
                    self.log_debug(format!("Right -> paginate to tx {}", self.sel_tx));
                }
            }
            2 => {
                // Scroll down 12 lines
                self.scroll_details(12);
                self.log_debug("Right -> scroll down 12 lines".into());
            }
            _ => {}
        }
    }

    pub fn page_up(&mut self, page: u16) {
        if self.pane == 2 {
            self.scroll_details(-(page as i32));
        }
    }
    pub fn page_down(&mut self, page: u16) {
        if self.pane == 2 {
            self.scroll_details(page as i32);
        }
    }
    pub fn home(&mut self) {
        if self.pane == 2 {
            self.details_home();
        }
    }
    pub fn end(&mut self) {
        if self.pane == 2 {
            // Jump to bottom using DetailsBuffer API
            self.details_end();
        }
    }

    fn scroll_details(&mut self, delta: i32) {
        // Delegate to DetailsBuffer scroll API
        self.scroll_details_lines(delta as isize);
    }

    /// Generic line scrolling based on current focused pane (for wheel events).
    /// Positive delta = down/next, negative delta = up/prev.
    pub fn scroll_lines(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }

        // Call up()/down() repeatedly to leverage existing navigation logic
        let steps = delta.abs();
        for _ in 0..steps {
            if delta > 0 {
                self.down();
            } else {
                self.up();
            }
        }
    }

    /// Request archival fetch for a block that's not in buffer or cache
    fn request_archival_block(&mut self, height: u64) {
        // Only request if we have archival fetch channel
        // Clone the sender to avoid borrow conflicts
        let tx = self.archival_fetch_tx.clone();
        if let Some(tx) = tx {
            // Only request if not already loading this block
            if self.loading_block != Some(height) {
                self.loading_block = Some(height);
                self.log_debug(format!("Requesting archival fetch for block #{height}"));
                if let Err(e) = tx.send(height) {
                    self.log_debug(format!("Failed to send archival fetch request: {e}"));
                    self.loading_block = None;
                }
            }
        }
    }

    pub fn select_tx(&mut self) {
        if self.current_block().is_some() {
            let (filtered_txs, _, _) = self.txs();
            if let Some(tx) = filtered_txs.get(self.sel_tx) {
                let cache_key = format!("tx_{}", tx.hash);

                // Check cache first to avoid expensive recursive parsing
                if let Some(cached) = self.parsed_json_cache.get(&cache_key) {
                    self.set_details_json(cached.clone());

                    // Trigger background fetch for full tx details if we have sender_id
                    // Only send if not already in flight (deduplication)
                    if let (Some(tx_details_tx), Some(sender_id)) =
                        (&self.tx_details_fetch_tx, &tx.signer_id)
                    {
                        if !self.tx_details_in_flight.contains(&tx.hash) {
                            self.tx_details_in_flight.insert(tx.hash.clone());
                            let _ = tx_details_tx.send((tx.hash.clone(), sender_id.clone()));
                        }
                    }

                    return;
                }

                // Cache miss - compute and store
                let mut val = serde_json::to_value(tx).unwrap_or(serde_json::Value::Null);
                val = crate::json_auto_parse::auto_parse_nested_json(val, 5, 0);
                let raw_json = crate::json_pretty::pretty_safe(&val, 2, 100 * 1024);

                self.parsed_json_cache.insert(cache_key, raw_json.clone());

                // Evict cache if too large (simple LRU: clear all)
                if self.parsed_json_cache.len() > 100 {
                    self.parsed_json_cache.clear();
                }

                self.set_details_json(raw_json);

                // Trigger background fetch for full tx details if we have sender_id
                // Only send if not already in flight (deduplication)
                if let (Some(tx_details_tx), Some(sender_id)) =
                    (&self.tx_details_fetch_tx, &tx.signer_id)
                {
                    if !self.tx_details_in_flight.contains(&tx.hash) {
                        self.tx_details_in_flight.insert(tx.hash.clone());
                        let _ = tx_details_tx.send((tx.hash.clone(), sender_id.clone()));
                    }
                }
            }
        }
    }

    /// Select first transaction, bypassing filter (for first block UX)
    pub fn select_tx_bypass_filter(&mut self) {
        // Clone the data we need before mutating self
        let block_data = self
            .current_block()
            .map(|b| (b.height, b.transactions.clone()));

        if let Some((_, all_txs)) = block_data {
            if let Some(tx) = all_txs.first() {
                self.sel_tx = 0;

                let cache_key = format!("tx_{}", tx.hash);

                // Check cache first to avoid expensive recursive parsing
                if let Some(cached) = self.parsed_json_cache.get(&cache_key) {
                    self.set_details_json(cached.clone());

                    // Trigger background fetch for full tx details if we have sender_id
                    // Only send if not already in flight (deduplication)
                    if let (Some(tx_details_tx), Some(sender_id)) =
                        (&self.tx_details_fetch_tx, &tx.signer_id)
                    {
                        if !self.tx_details_in_flight.contains(&tx.hash) {
                            self.tx_details_in_flight.insert(tx.hash.clone());
                            let _ = tx_details_tx.send((tx.hash.clone(), sender_id.clone()));
                        }
                    }

                    return;
                }

                // Cache miss - compute and store
                let mut val = serde_json::to_value(tx).unwrap_or(serde_json::Value::Null);
                val = crate::json_auto_parse::auto_parse_nested_json(val, 5, 0);
                let raw_json = crate::json_pretty::pretty_safe(&val, 2, 100 * 1024);

                self.parsed_json_cache.insert(cache_key, raw_json.clone());

                // Evict cache if too large
                if self.parsed_json_cache.len() > 100 {
                    self.parsed_json_cache.clear();
                }

                self.set_details_json(raw_json);

                // Trigger background fetch for full tx details if we have sender_id
                // Only send if not already in flight (deduplication)
                if let (Some(tx_details_tx), Some(sender_id)) =
                    (&self.tx_details_fetch_tx, &tx.signer_id)
                {
                    if !self.tx_details_in_flight.contains(&tx.hash) {
                        self.tx_details_in_flight.insert(tx.hash.clone());
                        let _ = tx_details_tx.send((tx.hash.clone(), sender_id.clone()));
                    }
                }
            } else {
                self.set_details_json("No transactions".to_string());
            }
        }
    }

    // ----- periodic tick for throttled backfill -----

    /// Called periodically from event loop to throttle backward chain-walk
    pub fn on_tick(&mut self, now: Instant) {
        self.maybe_step_backchain(now);
    }

    fn maybe_step_backchain(&mut self, now: Instant) {
        // Extract anchor block values we need (to avoid holding a borrow of self)
        let (anchor_height, anchor_prev_height, anchor_prev_hash) =
            if let Some(anchor) = self.current_block() {
                (anchor.height, anchor.prev_height, anchor.prev_hash.clone())
            } else {
                self.back_slots.clear();
                self.back_anchor_height = None;
                self.back_next_request_at = None;
                return;
            };

        // Anchor changed ⇒ reset the backward slots starting from its parent.
        if self.back_anchor_height != Some(anchor_height) {
            self.back_anchor_height = Some(anchor_height);
            self.back_slots.clear();
            self.back_next_request_at = None;

            if let (Some(prev_height), Some(ref prev_hash)) =
                (anchor_prev_height, &anchor_prev_hash)
            {
                self.back_slots.push(BackSlot {
                    height: prev_height,
                    hash: prev_hash.clone(),
                    state: BackSlotState::Pending,
                });
            } else {
                // Genesis or missing header metadata – nothing to backfill.
                return;
            }
        }

        if self.back_slots.len() >= self.back_slots_target {
            return;
        }

        // Simple throttle: at most one archival request per second.
        if let Some(next) = self.back_next_request_at {
            if now < next {
                return;
            }
        }

        // First, request data for any slot whose block we don't have yet.
        if let Some(slot) = self
            .back_slots
            .iter()
            .find(|slot| !self.is_block_available(slot.height))
        {
            self.request_archival_block(slot.height);
            self.back_next_request_at = Some(now + Duration::from_secs(1));
            return;
        }

        // All known slots have data; try to extend one more ancestor step.
        // Start with the anchor's prev pointers
        let mut prev_height = anchor_prev_height;
        let mut prev_hash = anchor_prev_hash;

        // Walk through each slot to get the deepest prev_height/prev_hash
        for slot in &self.back_slots {
            if let Some(b) = self.block_by_height(slot.height) {
                prev_height = b.prev_height;
                prev_hash = b.prev_hash.clone();
            } else {
                // We don't yet have this slot's block, so we can't walk further back.
                return;
            }
        }

        if let (Some(height), Some(hash)) = (prev_height, prev_hash) {
            if !self.back_slots.iter().any(|s| s.height == height)
                && self.back_slots.len() < self.back_slots_target
            {
                self.back_slots.push(BackSlot {
                    height,
                    hash,
                    state: BackSlotState::Pending,
                });
                self.back_next_request_at = Some(now + Duration::from_secs(1));
            }
        }
    }

    // ----- events -----
    pub fn on_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Quit => self.quit = true,
            AppEvent::FromWs(WsPayload::Block { data }) => {
                self.push_block(BlockRow {
                    height: data,
                    hash: "".into(),
                    prev_height: None,
                    prev_hash: None,
                    timestamp: 0,
                    tx_count: 0,
                    when: "".into(),
                    transactions: vec![],
                });
            }
            AppEvent::FromWs(WsPayload::Tx {
                identifier: _,
                data,
            }) => {
                if let Some(t) = data {
                    // For WS summary, show pretty-formatted JSON
                    let raw = serde_json::to_value(&t).unwrap_or(serde_json::json!({}));
                    self.set_details_json(pretty(&raw, 2));
                }
            }
            AppEvent::NewBlock(block) => {
                let height = block.height;

                if self.loading_block == Some(height) {
                    self.loading_block = None;
                }

                // If live updates are paused, drop blocks that are strictly in the future
                // of our current anchor. Historical backfill still flows through.
                if self.live_updates_paused {
                    if let Some(anchor) = self.current_block() {
                        if height > anchor.height {
                            self.log_debug(format!(
                                "[live-paused] dropping live block #{} (anchor #{})",
                                height, anchor.height
                            ));
                            return;
                        }
                    }
                }

                // While not paused, stop accepting live blocks that are "too far ahead"
                // of the selected anchor; the user can re-enable by pressing ← in the
                // Blocks pane.
                if !self.live_updates_paused {
                    let anchor_height = self.current_block().map(|b| b.height);
                    if let Some(anchor_h) = anchor_height {
                        let ahead = height.saturating_sub(anchor_h);
                        if ahead > FRONT_WINDOW {
                            self.live_updates_paused = true;
                            self.log_debug(format!(
                                "[live-paused] pausing at block #{} ({} ahead of anchor #{}) – press ← in Blocks to resume",
                                height, ahead, anchor_h
                            ));
                            return;
                        }
                    }
                }

                self.push_block(block);
            }
            AppEvent::FetchedTxDetails { tx_hash, json_data } => {
                // Transaction details fetched from archival RPC
                log::info!("[App] Received tx details for {}", tx_hash);

                // Store in full_tx_cache (with bounded LRU eviction)
                self.insert_full_tx(tx_hash.clone(), json_data.clone());

                // Remove from in-flight set (request completed)
                self.tx_details_in_flight.remove(&tx_hash);

                // Update Details pane with full archival JSON
                self.set_details_json(json_data);
            }
        }
    }

    fn push_block(&mut self, b: BlockRow) {
        let height = b.height;

        // Log state BEFORE push
        self.log_debug(format!(
            "[PUSH_START] Block #{}, follow_latest={}, sel_height={:?}, blocks_count={}",
            height,
            self.follow_blocks_latest,
            self.sel_block_height,
            self.blocks.len()
        ));

        // Determine if this is a historical block (older than current newest)
        let is_historical = self
            .blocks
            .first()
            .map(|newest| height < newest.height)
            .unwrap_or(false);

        if is_historical {
            // Historical block: insert at correct sorted position (descending height)
            let insert_pos = self
                .blocks
                .iter()
                .position(|existing| existing.height < height)
                .unwrap_or(self.blocks.len());

            self.blocks.insert(insert_pos, b);

            // Evict oldest if over limit
            if self.blocks.len() > self.keep_blocks {
                self.blocks.pop();
            }

            self.log_debug(format!(
                "[HISTORICAL_INSERT] Block #{} inserted at index {} (sorted position)",
                height, insert_pos
            ));
        } else {
            // Live streaming block: insert at front (newest position)
            self.blocks.insert(0, b);
            if self.blocks.len() > self.keep_blocks {
                // Remove oldest block
                self.blocks.pop();
            }
        }

        // Height-based selection behavior
        if self.follow_blocks_latest {
            // Auto-lock on FIRST block that passes filter (check if we've never selected before)
            if self.sel_block_height.is_none() {
                // Haven't locked to any block yet - check if this one passes filter
                let matching_txs = self.count_matching_txs(&self.blocks[0]);

                if matching_txs > 0 || filter::is_empty(&self.filter_compiled) {
                    // Block passes filter (or no filter active) - lock to it
                    self.sel_block_height = Some(height);
                    self.sel_tx = 0;
                    self.ensure_block_window_by_chain(height); // Chain-walk backfill on first selection
                    self.validate_and_refresh_tx(BlockChangeReason::AutoFollow);
                    self.follow_blocks_latest = false; // Disable auto-follow after first matching block
                    self.log_debug(format!(
                        "[FIRST_BLOCK] Block #{height} matches filter ({} txs), auto-selected and LOCKED",
                        matching_txs
                    ));
                } else {
                    // Block doesn't pass filter - stay in auto-follow mode
                    self.log_debug(format!(
                        "[SKIP_BLOCK] Block #{height} has no matching txs, waiting for next block"
                    ));
                }
            }
            // No else branch - subsequent blocks don't trigger auto-jump
        } else {
            // Manual mode: maintain locked block height (block may be in cache if aged out)
            if let Some(locked_height) = self.sel_block_height {
                if self.find_block_index(Some(locked_height)).is_some() {
                    // Block still in main buffer
                    self.log_debug(format!(
                        "Block #{height} arr, MANUAL mode locked to #{locked_height}"
                    ));
                } else if self.cached_blocks.contains_key(&locked_height) {
                    // Block aged out but available in cache
                    self.log_debug(format!("[MANUAL_CACHED] Block #{height} arr, MANUAL mode viewing cached block #{locked_height}"));
                } else {
                    // Block not in buffer or cache - shouldn't happen, but handle gracefully
                    self.log_debug(format!("[FALLBACK] Block #{height} arr, WARNING: locked block #{locked_height} not found, FORCING auto-follow"));
                    self.follow_blocks_latest = true; // Return to auto-follow mode
                    self.sel_block_height = None;
                    self.sel_tx = 0;
                    self.validate_and_refresh_tx(BlockChangeReason::AutoFollow);
                }
            }
        }

        // Keep archival window filled when pinned to a specific block
        if !self.follow_blocks_latest {}
    }

    // ----- Search methods -----
    pub fn start_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.search_query.clear();
        self.search_results.clear();
        self.search_selection = 0;
    }

    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    pub fn search_results(&self) -> &[crate::history::HistoryHit] {
        &self.search_results
    }

    pub fn search_selection(&self) -> usize {
        self.search_selection
    }

    pub fn search_add_char(&mut self, c: char) {
        self.search_query.push(c);
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
    }

    pub fn set_search_results(&mut self, results: Vec<crate::history::HistoryHit>) {
        self.search_results = results;
        self.search_selection = 0;
    }

    pub fn search_up(&mut self) {
        if self.search_selection > 0 {
            self.search_selection -= 1;
        }
    }

    pub fn search_down(&mut self) {
        if self.search_selection + 1 < self.search_results.len() {
            self.search_selection += 1;
        }
    }

    pub fn close_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search_query.clear();
        self.search_results.clear();
        self.search_selection = 0;
    }

    pub fn get_selected_search_result(&self) -> Option<&crate::history::HistoryHit> {
        self.search_results.get(self.search_selection)
    }

    pub fn display_tx_from_json(&mut self, raw_json: &str) {
        // Parse and display transaction from raw JSON
        if let Ok(tx) = serde_json::from_str::<serde_json::Value>(raw_json) {
            self.set_details_json(pretty(&tx, 2));
        }
    }

    // ----- Marks methods -----
    pub fn open_marks(&mut self, marks_list: Vec<crate::types::Mark>) {
        self.marks_list = marks_list;
        self.marks_selection = 0;
        self.input_mode = InputMode::Marks;
    }

    pub fn marks_list(&self) -> &[crate::types::Mark] {
        &self.marks_list
    }

    pub fn close_marks(&mut self) {
        self.input_mode = InputMode::Normal;
        self.marks_list.clear();
        self.marks_selection = 0;
    }

    pub fn marks_selection(&self) -> usize {
        self.marks_selection
    }

    pub fn marks_up(&mut self) {
        if self.marks_selection > 0 {
            self.marks_selection -= 1;
        }
    }

    pub fn marks_down(&mut self) {
        if self.marks_selection + 1 < self.marks_list.len() {
            self.marks_selection += 1;
        }
    }

    pub fn get_selected_mark(&self) -> Option<&crate::types::Mark> {
        self.marks_list.get(self.marks_selection)
    }

    pub fn current_context(&self) -> (u8, Option<u64>, Option<String>) {
        let pane = self.pane as u8;
        let height = self.current_block().map(|b| b.height);
        let tx_hash = if let Some(b) = self.current_block() {
            b.transactions.get(self.sel_tx).map(|t| t.hash.clone())
        } else {
            None
        };
        (pane, height, tx_hash)
    }

    pub fn jump_to_mark(&mut self, mark: &crate::types::Mark) {
        // Navigate to the mark's location
        if let Some(height) = mark.height {
            if self.blocks.iter().any(|b| b.height == height) {
                self.sel_block_height = Some(height); // Lock to specific block height
                self.follow_blocks_latest = false; // Jumping to mark disables auto-follow
                self.ensure_block_window_by_chain(height); // Chain-walk backfill
                self.validate_and_refresh_tx(BlockChangeReason::ManualNav);
            }
        }
        self.pane = mark.pane as usize;
        if self.pane > 2 {
            self.pane = 0;
        }
    }

    // ----- Web/egui helper methods -----

    /// Get count of blocks (for display)
    pub fn blocks_len(&self) -> usize {
        self.blocks.len()
    }

    /// Get count of transactions in current block
    pub fn txs_len(&self) -> usize {
        self.current_block().map_or(0, |b| b.transactions.len())
    }

    /// Get current block selection index (0-based)
    pub fn sel_block(&self) -> usize {
        if let Some(height) = self.sel_block_height {
            self.find_block_index(Some(height)).unwrap_or(0)
        } else {
            0 // Auto-follow mode: newest block is at index 0
        }
    }

    /// Select block by index with clamping
    pub fn select_block_clamped(&mut self, idx: usize) {
        if idx < self.blocks.len() {
            self.select_block_row(idx);
        }
    }

    /// Select transaction by index with clamping
    pub fn select_tx_clamped(&mut self, idx: usize) {
        if let Some(b) = self.current_block() {
            if idx < b.transactions.len() {
                self.select_tx_row(idx);
            }
        }
    }

    /// Set filter query and recompile
    pub fn set_filter_query(&mut self, query: String) {
        self.filter_query = query;
        self.filter_compiled = compile_filter(&self.filter_query);
        self.validate_and_refresh_tx(BlockChangeReason::FilterChange);
    }

    // ----- Details buffer API -----

    /// Set Details pane content (replaces full buffer)
    pub fn set_details_json(&mut self, json: String) {
        self.details_buf.set_text(json);
    }

    /// Set viewport size (called by renderer based on pane height)
    pub fn set_details_viewport_lines(&mut self, n: usize) {
        self.details_viewport_lines = n.max(1);
    }

    /// Get viewport size (for key handling)
    pub fn details_viewport_lines(&self) -> usize {
        self.details_viewport_lines
    }

    /// Get windowed view of Details (for rendering)
    pub fn details_window(&self) -> String {
        self.details_buf.window(self.details_viewport_lines)
    }

    /// Check if details content was truncated
    pub fn details_truncated(&self) -> bool {
        self.details_buf.truncated()
    }

    /// Get full Details text (for copy operations)
    pub fn details_full_text(&self) -> &str {
        self.details_buf.full_text()
    }

    /// Insert a full transaction JSON into the cache with LRU eviction
    fn insert_full_tx(&mut self, tx_hash: String, json: String) {
        // Update cache entry
        self.full_tx_cache.insert(tx_hash.clone(), json);

        // Update LRU order: remove if exists, then push to end (most recent)
        self.full_tx_order.retain(|k| k != &tx_hash);
        self.full_tx_order.push(tx_hash);

        // Evict oldest entries if cache exceeds MAX_FULL_TX
        while self.full_tx_order.len() > MAX_FULL_TX {
            if let Some(old_key) = self.full_tx_order.first().cloned() {
                self.full_tx_order.remove(0);
                self.full_tx_cache.remove(&old_key);
            }
        }
    }

    /// Get details as pretty-printed string (legacy compatibility)
    pub fn details_pretty_string(&self) -> String {
        self.details_buf.full_text().to_string()
    }

    /// Get details as raw JSON string (legacy compatibility)
    pub fn details_raw_string(&self) -> String {
        self.details_buf.full_text().to_string()
    }

    /// Scroll Details by delta lines
    pub fn scroll_details_lines(&mut self, delta: isize) {
        self.details_buf
            .scroll_lines(delta, self.details_viewport_lines);
    }

    /// Jump to top of Details
    pub fn details_home(&mut self) {
        self.details_buf.scroll_to_top();
    }

    /// Jump to bottom of Details
    pub fn details_end(&mut self) {
        self.details_buf
            .scroll_to_bottom(self.details_viewport_lines);
    }

    /// Get scroll info for status display
    pub fn details_scroll_info(&self) -> (usize, usize) {
        (
            self.details_buf.current_scroll_line(),
            self.details_buf.total_lines(),
        )
    }

    /// Get JSON for currently focused pane (for copy operation)
    pub fn focused_json_string(&mut self) -> Option<String> {
        Some(self.get_copy_content())
    }

    /// Get display-ready list of blocks (filtered if filter active)
    pub fn blocks_for_display(&self) -> Vec<&BlockRow> {
        let (blocks, _sel, _total) = self.filtered_blocks();
        blocks
    }

    /// Get display-ready list of transactions from current block (filtered)
    pub fn txs_for_display(&self) -> Vec<&TxLite> {
        if let Some(b) = self.current_block() {
            b.transactions
                .iter()
                .filter(|tx| {
                    // Apply text filter
                    if filter::is_empty(&self.filter_compiled) {
                        return true;
                    }
                    // Convert TxLite to JSON for filtering (same pattern as txs() method)
                    let v = json!({
                        "hash": tx.hash,
                        "signer_id": tx.signer_id.as_deref().unwrap_or(""),
                        "receiver_id": tx.receiver_id.as_deref().unwrap_or("")
                    });
                    tx_matches_filter(&v, &self.filter_compiled)
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get lightweight block data for display (owned copy)
    /// Returns None if index out of bounds
    pub fn block_lite(&self, idx: usize) -> Option<BlockLite> {
        self.blocks_for_display().get(idx).map(|b| BlockLite {
            height: b.height,
            tx_count: b.tx_count,
            when: b.when.clone(),
            time_utc: format_timestamp_utc(b.timestamp),
        })
    }

    /// Get lightweight transaction data for display (owned copy)
    /// Returns None if index out of bounds
    pub fn tx_lite(&self, idx: usize) -> Option<TxLite> {
        self.txs_for_display().get(idx).map(|tx| (*tx).clone())
    }

    /// Get count of filtered blocks
    pub fn filtered_blocks_len(&self) -> usize {
        self.blocks_for_display().len()
    }

    /// Get count of filtered transactions in current block
    pub fn filtered_txs_len(&self) -> usize {
        self.txs_for_display().len()
    }

    /// Handle mouse click at terminal coordinates
    pub fn handle_mouse_click(&mut self, col: u16, row: u16) {
        // Determine which pane was clicked based on layout
        // The layout is approximately:
        // - Header: 2 rows
        // - Filter bar: 1-3 rows (let's assume 2 average)
        // - Top 30%: Blocks (left 50%) + Txs (right 50%)
        // - Bottom 70%: Details

        // Skip header and filter (approximately 4 rows)
        if row < 4 {
            return;
        }

        // Get terminal size from last draw (this is approximate)
        // In real implementation, we'd pass the actual layout areas
        let term_height: u16 = 40; // Default assumption, will be resized dynamically
        let body_start: u16 = 4;
        let body_height = term_height.saturating_sub(body_start + 1); // -1 for footer
        let top_height = (body_height * 3) / 10; // 30%
        let top_end = body_start + top_height;

        if row < top_end {
            // We're in the top row
            if col < 60 {
                // Left half - Blocks pane
                self.pane = 0;
                // Calculate which block was clicked
                let block_idx = (row - body_start) as usize;
                if block_idx < self.blocks_len() {
                    self.select_block_clamped(block_idx);
                }
            } else {
                // Right half - Transactions pane
                self.pane = 1;
                // Calculate which tx was clicked
                let tx_idx = (row - body_start) as usize;
                if tx_idx < self.txs_len() {
                    self.select_tx_clamped(tx_idx);
                }
            }
        } else {
            // Bottom section - Details pane
            self.pane = 2;
        }
    }

    /// Check if coordinates are within details pane
    pub fn is_details_pane_at(&self, _col: u16, row: u16) -> bool {
        // Similar logic to handle_mouse_click
        // This is approximate - in real implementation we'd use actual layout
        let term_height: u16 = 40;
        let body_start: u16 = 4;
        let body_height = term_height.saturating_sub(body_start + 1);
        let top_height = (body_height * 3) / 10;
        let top_end = body_start + top_height;

        row >= top_end
    }

    /// Handle scroll wheel events
    pub fn handle_scroll(&mut self, col: u16, row: u16, lines: i32) {
        // First determine which pane we're scrolling in
        self.handle_mouse_click(col, row);

        // Then apply the scroll
        match self.pane {
            0 => {
                // Blocks pane
                let current = self.sel_block() as i32;
                let new_idx = (current + lines).max(0) as usize;
                self.select_block_clamped(new_idx);
            }
            1 => {
                // Transactions pane
                let current = self.sel_tx() as i32;
                let new_idx = (current + lines).max(0) as usize;
                self.select_tx_clamped(new_idx);
            }
            2 => {
                // Details pane - use DetailsBuffer scroll API
                self.scroll_details_lines(lines as isize);
            }
            _ => {}
        }
    }
}

/// Lightweight block data for display
#[derive(Debug, Clone)]
pub struct BlockLite {
    pub height: u64,
    pub tx_count: usize,
    pub when: String,     // Relative time ("2s ago")
    pub time_utc: String, // UTC timestamp
}

/// Format timestamp as UTC string
fn format_timestamp_utc(timestamp: u64) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use chrono::{TimeZone, Utc};
        let secs = (timestamp / 1_000_000_000) as i64;
        let nsecs = (timestamp % 1_000_000_000) as u32;
        match Utc.timestamp_opt(secs, nsecs) {
            chrono::LocalResult::Single(dt) => dt.format("%H:%M:%S").to_string(),
            _ => format!("{}s", secs),
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        // Simplified timestamp for WASM (just show seconds)
        let secs = (timestamp / 1_000_000_000) as i64;
        format!("{}s", secs)
    }
}

impl App {
    // UI Snapshot and Action handling are now centralized in ui_snapshot module
    // Use ui_snapshot::UiSnapshot::from_app(app) instead of app.ui_snapshot()
    // Use ui_snapshot::apply_ui_action(app, action) instead of app.handle_ui_action(action)
}
