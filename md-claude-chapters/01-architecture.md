# Chapter 1: Architecture

How the codebase is structured: Rust core, React frontend, platform abstraction, module inventory, and upstream sync.

## Two-Tier Build Reality

NEARx has two distinct build systems producing different artifacts:

1. **Rust** -- `nearx` (native TUI) and `nearxd` (broker daemon)
2. **React/Vite** -- `web/` (explorer website and Tauri webview frontend)

These share no compiled code at runtime. The Rust TUI uses ratatui for rendering; the web frontend is a standalone React app that talks to `nearxd` over unix socket (via Tauri commands) or directly to NEAR RPC.

## Rust Core

### Binaries

| Binary | Entry | Feature flag | Purpose |
|--------|-------|-------------|---------|
| `nearx` | `src/bin/nearx.rs` | `native` | Terminal UI (crossterm + ratatui) |
| `nearxd` | `src/bin/nearxd.rs` | `native` | Local broker daemon (unix socket JSON) |
| `nearx-proxy` | `src/bin/nearx-proxy.rs` | `native` | RPC proxy utility |

### Key Modules

| Module | Purpose |
|--------|---------|
| `src/app.rs` | Core application state, route application (`apply_route`) |
| `src/ui.rs` | Ratatui rendering (TUI layout, panes, overlays) |
| `src/filter.rs` | Query grammar for transaction filtering |
| `src/router.rs` | Deep-link parser and canonicalizer |
| `src/theme.rs` | Color palette, WCAG-compliant theme definitions |
| `src/source_rpc.rs` | NEAR RPC polling with catch-up limits |
| `src/source_ws.rs` | WebSocket connection to Node server |
| `src/history.rs` | SQLite-backed transaction history (native only) |
| `src/archival_fetch.rs` | On-demand historical block fetching |
| `src/flags.rs` | UI feature toggles (`UiFlags`) |
| `src/platform/` | Platform abstraction (clipboard, storage, runtime) |

### Feature Flags

```toml
[features]
default = []
native = [
    "dep:crossterm", "dep:copypasta", "dep:rusqlite", "dep:notify",
    "dep:tokio-tungstenite", "dep:tungstenite", "dep:futures-util",
    "dep:near-primitives", "dep:near-crypto", ...
    "tokio/rt-multi-thread", "tokio/macros", "tokio/time", "tokio/signal", ...
]
```

The `native` feature pulls in platform-specific dependencies (SQLite, WebSocket, NEAR SDK C libraries). No default features -- explicit selection required.

### Design Principles

- **FPS-capped rendering** -- coalesced draws (default 30 FPS) prevent UI thrashing
- **Non-blocking I/O** -- all data fetching happens off the render thread
- **Catch-up limits** -- RPC mode limits blocks per poll to prevent cascade failures
- **Height-based selection** -- block selection tracks by height, not index, for stability during updates

## Web Frontend (React/TypeScript)

### Stack

- **React 18** with TypeScript
- **Tailwind CSS v4** for styling
- **Vite** for dev server and build
- **react-router-dom** for routing

### Entry Point

`web/src/main.tsx` -- standard React app, no WASM involved.

### Build

```bash
yarn workspace explorer-frontend build   # production
yarn workspace explorer-frontend dev     # dev server
```

Output: `web/dist/`

### Tauri Integration

When running inside Tauri (`VITE_TAURI=true`), the frontend imports from `@tauri-apps/api` and `@tauri-apps/plugin-deep-link`:

- `web/src/tauri/runtime.ts` -- invoke Tauri commands (`get_runtime_config`, `sign_transaction`, etc.)
- `web/src/tauri/deeplink.ts` -- listen for deep-link events from the Tauri shell

### NEARx-Only Additions

Files that exist only in NEARx (not in upstream `fastnear/explorer-frontend`):

- `src/tauri/` -- Tauri integration (runtime, deeplink)
- `src/pages/SignTransaction.tsx` -- transaction signing UI
- `src/pages/Staking.tsx` -- staking dashboard
- `src/components/AccountPicker.tsx` -- account selection
- `src/components/Sidebar.tsx` -- navigation sidebar
- `src/hooks/useAccountPrefs.ts` -- account preferences
- `src/api/staking.ts` -- staking API calls
- `src/utils/networkRouting.ts` -- network-aware routing

These are listed in `web/.explorer-upstream.json` under `nearx_only` and excluded from upstream comparison.

## Upstream Sync

The `web/` directory is a superset of [`fastnear/explorer-frontend`](https://github.com/fastnear/explorer-frontend). Shared files should stay in sync.

### Config

`web/.explorer-upstream.json` tracks:
- `synced_sha` -- last synced upstream commit
- `nearx_only` -- paths excluded from comparison

### Workflow

```bash
tools/sync-explorer.sh              # summary: identical / diverged / missing
tools/sync-explorer.sh --latest     # compare against upstream HEAD
tools/sync-explorer.sh --diff       # show full diffs
tools/sync-explorer.sh --apply      # interactive per-file overwrite
tools/sync-explorer.sh --bump       # update synced_sha after resolving
```

When adding new NEARx-only files under `web/`, add them to the `nearx_only` list.

## Workspace Structure

```
nearx/
├── package.json             # Yarn 4 workspace root (workspaces: web, e2e-tests)
├── Cargo.toml               # Rust workspace
├── src/                     # Shared Rust core crate
│   ├── bin/
│   │   ├── nearx.rs         # Native TUI
│   │   ├── nearxd.rs        # Broker daemon
│   │   └── nearx-proxy.rs   # RPC proxy
│   ├── app.rs               # Application state
│   ├── ui.rs                # Ratatui rendering
│   ├── router.rs            # Deep-link parser
│   └── ...
├── web/                     # React frontend (synced from explorer-frontend)
│   ├── src/
│   │   ├── main.tsx
│   │   ├── tauri/           # Tauri integration
│   │   ├── pages/
│   │   └── components/
│   └── package.json         # "explorer-frontend"
├── native-host/             # Browser native messaging host
├── extension/               # Browser extension (MV3)
├── tauri-workspace/         # Tauri desktop shell
│   └── src-tauri/
│       ├── src/main.rs
│       └── tauri.conf.json
├── e2e-tests/               # Playwright E2E suite
├── archive/legacy-web-dom/  # Archived WASM/DOM build (historical)
└── tools/                   # Build scripts (sidecar, sync)
```

## Platform Abstraction (Rust)

The `src/platform/` module provides unified interfaces for the native TUI:

- **Clipboard** -- terminal-specific copy
- **Storage** -- SQLite persistence
- **File access** -- credential file watching
- **Runtime** -- full tokio with multi-thread

The web frontend does not use this layer; it has its own clipboard, storage (localStorage), and network code.
