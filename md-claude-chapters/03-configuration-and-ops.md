# Chapter 3: Configuration & Operations

Environment variables, CLI args, token handling, build commands, and troubleshooting.

## Prerequisites

- **Rust** 1.70+ via [rustup](https://rustup.rs/)
- **Node.js** 20+ with Yarn 4 (via corepack: `corepack enable`)
- **Tauri** prerequisites: see [tauri.app/start/prerequisites](https://v2.tauri.app/start/prerequisites/)

```bash
git clone https://github.com/fastnear/nearx.git && cd nearx
yarn install        # web frontend + e2e dependencies
```

## Configuration Priority

**CLI args > Environment variables > Defaults**

### Data Source

| Variable | CLI | Default | Description |
|----------|-----|---------|-------------|
| `SOURCE` | `--source, -s` | `ws` | `ws` (WebSocket) or `rpc` (polling) |

### RPC Settings

| Variable | CLI | Default | Description |
|----------|-----|---------|-------------|
| `NEAR_NODE_URL` | `--near-node-url` | `https://rpc.testnet.fastnear.com/` | RPC endpoint |
| `FASTNEAR_AUTH_TOKEN` | `--fastnear-auth-token` | -- | API token (avoid rate limits) |
| `POLL_INTERVAL_MS` | `--poll-interval-ms` | `1000` | Polling interval (100-10000ms) |
| `POLL_MAX_CATCHUP` | `--poll-max-catchup` | `5` | Max blocks per poll (1-100) |
| `POLL_CHUNK_CONCURRENCY` | `--poll-chunk-concurrency` | `4` | Concurrent fetches (1-16) |
| `RPC_TIMEOUT_MS` | `--rpc-timeout-ms` | `8000` | Request timeout (1000-60000ms) |
| `RPC_RETRIES` | `--rpc-retries` | `2` | Retry attempts (0-10) |
| `ARCHIVAL_RPC_URL` | `--archival-rpc-url` | -- | Archival endpoint for historical blocks |

### WebSocket Settings

| Variable | CLI | Default | Description |
|----------|-----|---------|-------------|
| `WS_URL` | `--ws-url` | `ws://127.0.0.1:63736` | WebSocket endpoint |
| `WS_FETCH_BLOCKS` | `--ws-fetch-blocks` | `true` | Fetch full block data |

### UI Performance

| Variable | CLI | Default | Description |
|----------|-----|---------|-------------|
| `RENDER_FPS` | `--render-fps` | `30` | Target FPS (1-120) |
| `RENDER_FPS_CHOICES` | `--render-fps-choices` | `20,30,60` | Cycle options (Ctrl+O) |
| `KEEP_BLOCKS` | `--keep-blocks` | `100` | Blocks in memory (10-10000) |

### Persistence & Credentials

| Variable | Default | Description |
|----------|---------|-------------|
| `SQLITE_DB_PATH` | `./nearx_history.db` | Transaction history database |
| `NEAR_CREDENTIALS_DIR` | `$HOME/.near-credentials` | Credential files |
| `NEAR_NETWORK` | `mainnet` | Network subdirectory |

### Default Filtering

| Variable | CLI | Default | Description |
|----------|-----|---------|-------------|
| `WATCH_ACCOUNTS` | `--watch-accounts` | `intents.near` | Comma-separated account list |
| `DEFAULT_FILTER` | `--default-filter` | `acct:intents.near` | Filter syntax (if WATCH_ACCOUNTS unset) |

## Token Handling

### nearxd Resolution Chain

The `nearxd` broker resolves API tokens in this order:

1. In-memory session token (set via `set_fastnear_auth_token`)
2. Persisted token store backend (keychain or file)
3. `FASTNEAR_API_KEY` env (canonical)
4. `FASTNEAR_AUTH_TOKEN` env (legacy alias)

Tokens are appended as `?apiKey=<KEY>` on FastNear RPC/API URLs.

### Token Backend Selection (`NEARXD_TOKEN_BACKEND`)

| Value | Behavior |
|-------|----------|
| `auto` (default) | macOS: keychain + file fallback; other: file |
| `keychain` | macOS keychain only |
| `file` | File at `NEARXD_TOKEN_FILE` or `~/.nearx/fastnear_auth_token` |

### Web Frontend Token

The web frontend uses `VITE_API_BASE_URL` for API endpoint configuration. In Tauri mode, runtime config (including token) is fetched from `nearxd` via the `get_runtime_config` command.

## Build Commands

### Native TUI

```bash
cargo build --bin nearx --features native --release
cargo run --bin nearx --features native -- --source rpc
```

### nearxd Broker

```bash
cargo build --bin nearxd --features native --release
```

### Web Frontend

```bash
yarn workspace explorer-frontend build    # production build -> web/dist/
yarn workspace explorer-frontend dev      # dev server on :5173
```

### Tauri Desktop

```bash
bash tools/build-sidecar.sh              # build nearxd sidecar (required first)
cd tauri-workspace && cargo tauri dev     # dev mode (starts Vite + Tauri)
cd tauri-workspace && cargo tauri build   # production build
```

### Native Host

```bash
cargo build --manifest-path native-host/Cargo.toml --release
```

## Verification Commands

```bash
# Rust checks
cargo check --bin nearx
cargo test --lib
cargo check --bin nearxd
cargo test --bin nearxd
cargo check --manifest-path native-host/Cargo.toml
cargo test --manifest-path native-host/Cargo.toml
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml --features e2e

# Web frontend typecheck
cd web && npx tsc -b

# E2E tests
cd e2e-tests && npm test     # Tauri E2E
yarn e2e                     # Playwright web E2E
```

## Troubleshooting

### Connection Issues

**WebSocket refused** (`SOURCE=ws`): ensure Node server is running on port 63736, check `WS_URL`.

**RPC timeouts**: increase timeout and reduce concurrency:
```bash
RPC_TIMEOUT_MS=15000 POLL_CHUNK_CONCURRENCY=2 cargo run --bin nearx --features native
```

### Performance

**High CPU**: reduce FPS and block count:
```bash
RENDER_FPS=20 KEEP_BLOCKS=50 cargo run --bin nearx --features native
```

### Web Build

**TypeScript errors**: run `cd web && npx tsc -b` to see specific issues.

**Vite dev server port conflict**: the Tauri config uses `--port 1420 --strictPort`; standalone dev uses default `:5173`.

### nearxd

**Socket already in use**: check for stale socket file at `${TMPDIR:-/tmp}/nearxd.sock`. A running `nearxd` instance may already be listening.

**Token not resolving**: check the resolution chain -- session token, then persisted store, then env vars. Use `NEARXD_TOKEN_BACKEND=file` to force file-based storage for debugging.
