# Changelog

All notable changes to NEARx will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.5] - November 2025

### Added
- JSON streaming architecture with DetailsBuffer supporting 5000 line truncation
- Scroll position indicators "(42/1234)" format in both TUI and web
- Truncation messages when content exceeds 5000 lines
- DOS-ish CSS styling for web/Tauri targets

### Fixed
- JSON final bracket rendering issue in TUI
- Viewport-aware scrolling bounds in DetailsBuffer
- Standardized title formatting across all targets
- Removed orphaned archive/ directory

## [0.4.4] - November 2025

### Added
- Two-list block architecture with automatic backfill placeholders
- Unified copy behavior across all targets (TUI/Web/Tauri)
- Vertical centering for block selection in web UI
- Three visual states for blocks: Forward, BackfillPending, BackfillLoading

### Changed
- All targets now route copy through `apply_ui_action()` for consistency
- Toast messages shortened: "Copied block", "Copied transaction", "Copied details"

### Fixed
- Seamless infinite scrolling through blockchain history
- WASM backfill chain support with prev_height parsing

## [0.4.3] - November 2025

### Added
- Pure DOM frontend replacing egui/canvas
- Fullscreen dual-mode navigation (Scroll/Navigate modes)
- JSON bridge architecture (UiSnapshot/UiAction)
- Headless App pattern for web targets

### Removed
- All egui dependencies from web builds
- wasm_guard.js error masking
- Canvas/WebGL rendering

### Changed
- Web binary from `nearx-web` to `nearx-web-dom`
- Build uses `--no-default-features --features dom-web`

## [0.4.2] - November 2025

### Added
- OAuth & Authentication (Google OAuth + Magic links)
- Unified theme system (WCAG AA compliant)
- Full mouse/keyboard parity across all targets
- XSS-hardened CSP security headers
- Router shim for auth callbacks
- Smart Esc key handling (priority-based)

### Security
- Token handling with URL scrubbing
- CSP headers blocking XSS attacks
- System browser isolation for OAuth (Tauri)

## [0.4.1] - October 2025

### Added
- Unified clipboard system with 4-tier fallback chain
- Mouse wheel scrolling for Web/Tauri
- Platform abstraction layer
- Tauri clipboard plugin integration

### Fixed
- Clipboard code duplication across binaries
- Maximum compatibility across all environments

## [0.4.0] - October 2025

### Added
- Quad-mode architecture (Terminal, Web, Tauri, Browser Extension)
- Browser extension with Native Messaging host
- Production-ready browser integration

## [0.3.0] - 2025

### Block Selection Refactor
- Height-based block tracking (was index-based)
- Auto-follow mode with manual navigation
- Intelligent transaction selection preservation

### Smart Block Filtering
- Blocks panel shows only blocks with matching transactions when filtered
- Clear visual feedback with "Blocks (12 / 100)" count display
- Navigation follows filtered list

### Archival RPC Support
- Unlimited backward navigation through blockchain history
- On-demand fetching from archival endpoints
- Loading state indicators

### Function Call Arguments Decoding
- Three-tier decoding: JSON → Printable Text → Binary Hex Dump
- Auto-parsing nested JSON strings
- Full decoded args for all action types

### Delegate Action Support
- Full recursive parsing of nested actions
- Formatted display with method names, amounts, gas

### UI Optimizations
- 70/30 layout split for better space usage
- No left border on details pane for easy selection
- Mouse wheel scrolling (Web/Tauri)
- Smart Esc handling
- Dynamic chrome
- Toast notifications
- Ratio-based layouts

### Configuration System
- CLI argument support
- Priority chain: CLI > Env > Defaults
- Comprehensive validation
- 147-line .env.example template

### Jump Marks System
- Persistent bookmarks
- Pinning support
- Quick jump navigation
- SQLite persistence

### Owned Accounts Filtering
- Auto-discovery from ~/.near-credentials
- Ctrl+U toggle
- Real-time file watching

### Context-Aware Block Caching
- ±50 blocks around selection
- LRU eviction (300 blocks max)
- Visual indicators for cached blocks

### Filter UX Improvements
- Filtered count display
- Default filtering via WATCH_ACCOUNTS
- Simple account watching