# Chapter 5: Building

This chapter covers building NEARx for different platforms: native terminal, web browser (WASM), and Tauri desktop.

## Native Terminal Mode

### Font Rendering Note
The native terminal version uses your terminal emulator's font settings. NEARx does not control font rendering - this is managed by your terminal emulator (iTerm2, Alacritty, Terminal.app, etc.).

### Recommended Monospace Fonts
- **JetBrains Mono** - Excellent Unicode coverage, designed for code
- **Cascadia Code** - Microsoft's modern terminal font with ligatures
- **Fira Code** - Popular with programmers, good ligature support
- **SF Mono** (macOS) - Apple's system monospace font
- **Menlo** (macOS) - Classic Mac terminal font

### Build Commands

```bash
# Build release version (requires native feature)
cargo build --bin nearx --features native --release

# Run with default settings (WebSocket mode)
cargo run --bin nearx --features native

# Run in RPC mode
cargo run --bin nearx --features native -- --source rpc

# Run with custom settings
cargo run --bin nearx --features native -- --source rpc --render-fps 60 --keep-blocks 200

# Or use environment variables
SOURCE=rpc RENDER_FPS=60 cargo run --bin nearx --features native

# View all CLI options
cargo run --bin nearx --features native -- --help

# Run release binary directly
./target/release/nearx --source rpc --near-node-url https://rpc.mainnet.fastnear.com/
```

## Web Browser Mode (DOM Frontend)

### Technology Stack
Pure DOM-based frontend using native HTML/CSS/JavaScript with WASM core. No canvas or WebGL - just regular web elements for maximum compatibility and native UX.

### Architecture
**Headless App pattern with JSON bridge:**
- **Rust (WASM)**: `WasmApp` exposes `App` via JSON snapshots (`UiSnapshot`) and actions (`UiAction`)
- **JavaScript**: DOM renderer consumes snapshots, dispatches user actions
- **Data Flow**: RPC events → App state → JSON snapshot → DOM render → User action → App update

### Prerequisites

```bash
# Install wasm-bindgen-cli (WASM bindings generator)
cargo install wasm-bindgen-cli --locked

# Add WASM target
rustup target add wasm32-unknown-unknown
```

### Build Commands

```bash
# Development (builds and serves at http://localhost:8000)
make dev

# Debug build (WASM output to web/pkg/)
make web

# Production build
make web-release

# Clean build artifacts
make clean
```

### Critical Build Details
- **Binary**: `nearx-web-dom` (specified in Makefile)
- **HTML**: `web/index.html` (clean DOM structure, no canvas)
- **Features**: `--no-default-features --features dom-web` (pure WASM dependencies, **zero egui**)
- **Output**: `web/pkg/` directory (gitignored WASM artifacts)

### FastNEAR Token Configuration

For web and Tauri builds, the token handling uses a **priority fallback chain**:

1. **OAuth token** (highest priority): User's authentication token from localStorage (set via OAuth login)
2. **Compile-time token** (fallback): `FASTNEAR_API_TOKEN_WEB` or `FASTNEAR_API_TOKEN` environment variable baked into WASM at build time

**Local Development Setup:**
```bash
# Set token before building (persists for current shell session)
export FASTNEAR_API_TOKEN_WEB="your-token-here"

# Build web bundle
make web

# Serve and test
cd web
python -m http.server 8000
# Open http://localhost:8000
```

**Production Deployment:**
- **Public web**: Leave `FASTNEAR_API_TOKEN_WEB` unset (avoids embedding secrets in public WASM). Rely on OAuth or run unauthenticated.
- **Internal/private**: Set `FASTNEAR_API_TOKEN_WEB` during build for embedded token.

### JSON Bridge API

The web frontend communicates with the WASM core via JSON serialization:

**Rust → JavaScript (UiSnapshot):**
- Pane focus state
- Filter bar state
- Blocks/Transactions/Details data
- Toast notifications
- Loading states
- Auth state

**JavaScript → Rust (UiAction):**
- Navigation commands (arrow keys, page up/down)
- Pane switching (Tab/Shift+Tab)
- Filter updates
- Copy commands
- Auth actions

### Common Build Errors & Solutions

1. **Error: `winit` not supported on this platform**
   - **Cause:** Trying to build for native target instead of wasm32
   - **Fix:** Use `cargo check --target wasm32-unknown-unknown` or let Makefile handle it

2. **Error: wasm-bindgen version mismatch**
   - **Cause:** CLI version doesn't match wasm-bindgen crate version
   - **Fix:** Reinstall with exact version: `cargo install wasm-bindgen-cli --version 0.2.104 --locked --force`

### Verifying the Build

```bash
# Clean build
make clean
make web

# Check WASM target compiles without warnings
cargo check --bin nearx-web-dom --target wasm32-unknown-unknown \
  --no-default-features --features dom-web

# Verify web/pkg/ structure
ls web/pkg/
# Should show: nearx-web-dom.js, nearx-web-dom_bg.wasm, snippets/

# Test dev server
make dev
# Open http://localhost:8000 in browser
```

## Important Note: Documentation Inconsistency

The CLAUDE.md file contains outdated information about "web builds requiring ratatui 0.29+ for egui_ratatui compatibility". This is **incorrect** for the current DOM-based web build which uses no egui at all. This confusion may be related to the performance issues being experienced.

The current web build:
- Uses `nearx-web-dom` binary
- Has **zero egui dependencies**
- Uses pure DOM rendering with JSON bridge
- Should not be affected by any egui-related configuration

## Tauri Desktop App Mode

### Overview
Native desktop application with deep link support for handling `nearx://` URLs. Built with Tauri v2, combining Rust backend with **DOM frontend** (same static site as web).

### Frontend
Uses the same `web/` static site as the web build - pure DOM with WASM core, no canvas or WebGL. Configuration in `tauri.conf.json` points to `frontendDist: "../../web"`.

### Build Requirements
Tauri automatically builds the frontend via Makefile (see `tauri.conf.json` → `beforeDevCommand` and `beforeBuildCommand`).

### Build Commands

```bash
# Development (automatically runs 'make dev')
cd tauri-workspace
cargo tauri dev

# Production build (automatically runs 'make web-release')
cd tauri-workspace
cargo tauri build
```

## Platform Feature Matrix

| Feature | Native Terminal | Web Browser | Tauri Desktop |
|---------|----------------|-------------|---------------|
| Data Sources | WebSocket + RPC | RPC only | RPC only |
| Storage | SQLite | In-memory | In-memory |
| History Search | ✓ | ✗ | ✗ |
| Jump Marks | ✓ | ✗ | ✗ |
| Clipboard | Terminal-specific | Browser API | Tauri plugin |
| Deep Links | ✗ | ✗ | ✓ |
| OAuth | ✗ | ✓ | ✓ |
| File Watching | ✓ | ✗ | ✗ |

## Next Steps

- For Tauri-specific details, see [Chapter 6: Tauri Desktop](06-tauri-desktop.md)
- For testing procedures, see [Chapter 7: Testing & Security](07-testing-security.md)
- For troubleshooting, see [Chapter 8: Reference](08-reference.md)