# Chapter 4: Architecture

This chapter covers the core architecture of NEARx, including design principles, component organization, and the platform abstraction layer.

## Key Design Principles

1. **FPS-Capped Rendering** - Coalesced draws (default 30 FPS) prevent UI thrashing
2. **Non-Blocking I/O** - All data fetching and persistence happens off the main render thread
3. **Catch-Up Limits** - RPC mode limits blocks per poll to prevent cascade failures
4. **Async Everything** - Tokio-based async runtime keeps UI responsive
5. **Soft-Wrapped Tokens** - ZWSP insertion for clean line breaking of long hashes
6. **Feature Toggles** - UI enhancements gated by runtime flags for easy rollback if needed

## UI Feature Toggles (`UiFlags`)

NEARx uses a feature flag system to control enhanced UI behaviors introduced for Web/Tauri targets. This allows quick disable of new features without code surgery.

### Available Flags

```rust
pub struct UiFlags {
    /// Consume Tab/Shift+Tab after cycling panes (prevents egui focus hijack)
    /// Default: true (stable, production-ready)
    pub consume_tab: bool,

    /// Snap egui pixels_per_point to devicePixelRatio (crisp Hi-DPI rendering)
    /// Default: true (stable, production-ready)
    pub dpr_snap: bool,

    /// Mouse/trackpad click mapping to pane focus + row select
    /// Default: true (stable, production-ready)
    pub mouse_map: bool,

    /// Double-click in Details toggles fullscreen overlay
    /// Default: true (stable, production-ready)
    pub dblclick_details: bool,
}
```

### Usage Examples

**Default behavior (production-safe):**
```rust
let app = App::new(fps, fps_choices, keep_blocks, filter, archival_tx);
// consume_tab=true, dpr_snap=true, mouse_map=true, dblclick_details=true
```

**Disable all enhancements (maximum stability):**
```rust
let mut app = App::new(fps, fps_choices, keep_blocks, filter, archival_tx);
app.set_ui_flags(UiFlags::all_disabled());
```

### Implementation Details

- **Location**: `src/flags.rs` (module), `src/app.rs` (App field)
- **Gating**: Web binary checks flags before applying behaviors
- **Runtime Toggleable**: Can be changed via `app.set_ui_flags()` at any time
- **Zero Cost**: Flags are copied on each use (cheap), no performance impact
- **Native Unaffected**: Feature flags exist on native builds but aren't used (TUI has no egui)

## Core Components

### Data Sources (`src/source_*.rs`)

**WebSocket Mode** (`source_ws.rs`):
- Connects to Node breakout server on port 63736
- Real-time block and transaction events
- Ideal for development alongside your Node server

**RPC Mode** (`source_rpc.rs`):
- Direct NEAR RPC polling with smart catch-up
- Non-overlapping polls with configurable limits
- Concurrent chunk fetching (default 4 parallel requests)
- Ideal for production monitoring

**Archival Fetch** (`archival_fetch.rs` / `archival_fetch_wasm.rs`):
- Background task for fetching historical blocks beyond the rolling buffer
- On-demand fetching via channel communication
- Reuses existing `fetch_block_with_txs()` RPC infrastructure
- Enables unlimited backward navigation through blockchain history
- Optional: only runs if `ARCHIVAL_RPC_URL` is configured
- WASM version uses browser Fetch API for non-blocking requests

### Application State (`src/app.rs`)

The core `App` struct manages all application state:

```rust
pub struct App {
    // Core state - height-based block tracking for stable selection
    blocks: Vec<BlockRow>,
    sel_block_height: Option<u64>,  // None = auto-follow newest, Some(height) = locked
    sel_tx: usize,
    manual_block_nav: bool,         // Whether user navigated away from newest
    details: String,

    // Block caching (preserves ±50 blocks around selection after aging out)
    cached_blocks: HashMap<u64, BlockRow>,
    cached_block_order: Vec<u64>,   // LRU tracking (max 300 blocks)

    // Archival fetch state (for fetching historical blocks beyond cache)
    loading_block: Option<u64>,                          // Block height currently being fetched
    archival_fetch_tx: Option<UnboundedSender<u64>>,    // Channel to request archival fetches

    // Filter state
    filter_query: String,
    filter_compiled: CompiledFilter,

    // Search state
    search_query: String,
    search_results: Vec<HistoryHit>,
    search_selection: usize,

    input_mode: InputMode,  // Normal | Filter | Search | Marks | JumpPending

    // Jump marks for navigation
    marks_list: Vec<Mark>,
    marks_selection: usize,

    // Owned accounts filtering (from ~/.near-credentials)
    owned_accounts: HashSet<String>,
    owned_only_filter: bool,

    // Performance
    fps: u32,
    fps_choices: Vec<u32>,

    // Debug panel (Ctrl+D)
    debug_log: Vec<String>,
    debug_visible: bool,
}
```

### UI Rendering (`src/ui.rs`)

Two-row layout optimized for readability:
- **Header**: Tab-style pane selector with focus indicators
- **Filter Bar**: Dynamic height (collapses to 1 line when idle, expands to 3 when active)
- **Body**:
  - Top row (30% height): Blocks (left 50%) + Transaction hashes (right 50%)
  - Bottom row (70% height): Details pane (full width)
- **Debug Panel**: Toggleable with Ctrl+D (shows navigation events)
- **Footer**: Keyboard shortcuts + FPS indicator + owned filter status + pinned marks count + toast notifications
- **Search Overlay**: Centered modal for history search (Ctrl+F)
- **Marks Overlay**: Navigation bookmarks list (Shift+M)

**Layout Ratios**: Uses `Constraint::Ratio(3,10)` and `Ratio(7,10)` for precise 30/70 split, eliminating rounding gaps from percentage-based constraints.

**Details Pane Design**:
- No left or right borders (`Borders::TOP` only)
- No horizontal padding (`left: 0, right: 0`)
- One line of top spacing (`top: 1`) for visual breathing room
- Enables easy text selection without fighting borders

### Filter System (`src/filter.rs`)

Query grammar for real-time transaction filtering:
```
acct:alice.near       # Match signer OR receiver
signer:bob.near       # Match signer only
receiver:contract     # Match receiver only
action:FunctionCall   # Match action type
method:ft_transfer    # Match method name
raw:some_text         # Search in raw JSON
freetext              # Match anywhere
```

All filters use AND logic; within each field type, OR logic applies.

### History Search (`src/history.rs`)

Off-thread SQLite persistence with async search:
- Non-blocking writes via `spawn_blocking`
- WAL mode for concurrent reads
- Indexed on signer, receiver, height
- Query builder with LIKE-based search
- Prepared for FTS5 upgrade

## Project Structure

```
nearx/
├── Cargo.toml           # Dependencies with feature flags (native/web)
├── Makefile             # Web build automation
├── web/                 # Web frontend assets
│   ├── index.html       # DOM frontend entry point
│   ├── app.js           # DOM renderer (snapshot → render → action)
│   ├── theme.css        # Theme variables
│   ├── platform.js      # Unified clipboard bridge
│   ├── auth.js          # OAuth popup manager
│   └── router_shim.js   # Hash change router
├── src/
│   ├── lib.rs           # Library exports (shared core)
│   ├── bin/             # Platform-specific binaries
│   │   ├── nearx.rs     # Native terminal binary
│   │   └── nearx-web-dom.rs # DOM frontend binary (WASM)
│   ├── platform/        # Platform abstraction layer
│   ├── app.rs           # Application state (shared)
│   ├── ui.rs            # Ratatui rendering (shared)
│   ├── ui_snapshot.rs   # JSON bridge for Web/Tauri
│   └── [other modules]  # Various functionality
└── tauri-workspace/     # Tauri desktop app
```

### Key Architectural Decisions

- **Library-first design**: Core logic in `lib.rs`, platform-specific in `bin/`
- **Feature flags**: `native` vs `web` enable/disable platform-specific code
- **Conditional compilation**: `#[cfg(feature = "native")]` for native-only modules
- **Platform abstraction**: `platform/` module provides unified interface
- **Shared core App**: Same `App` state engine used across all targets
- **DOM frontend**: Pure HTML/CSS/JS with JSON bridge to WASM core
- **Headless pattern**: App exposed via `UiSnapshot` (state) and `UiAction` (commands) JSON API

## Platform Abstraction

The `platform/` module provides a unified interface for platform-specific functionality:

- **Clipboard**: 4-tier fallback chain (Tauri → Extension → Navigator → execCommand)
- **Storage**: SQLite (native) vs in-memory (web)
- **File access**: Credentials watching (native only)
- **Runtime**: Full tokio (native) vs WASM-compatible subset (web)

This abstraction allows the core application logic to remain platform-agnostic while still leveraging native capabilities where available.

## Next Steps

- For building instructions, see [Chapter 5: Building](05-building.md)
- For Tauri desktop details, see [Chapter 6: Tauri Desktop](06-tauri-desktop.md)
- For testing and security, see [Chapter 7: Testing & Security](07-testing-security.md)
