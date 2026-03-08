# NEARx

NEARx is a NEAR Protocol explorer with a shared Rust core, a local broker daemon, and a React/Vite frontend used by both the standalone web target and Tauri desktop app.

## Run (Tauri Desktop)

```bash
yarn install                    # JS deps (first time / after pull)
node tools/build-sidecar.mjs    # nearxd sidecar (first time / after Rust changes)
cd tauri-workspace && cargo tauri dev
```

That's it. `cargo tauri dev` starts the Vite dev server and spawns `nearxd` as a managed sidecar — no separate terminals. If a standalone `nearxd` is already running on the default socket, the sidecar is skipped and the existing instance is reused.

## How the Pieces Fit Together

```text
┌─────────────────────────────────────────────────────────────┐
│  Tauri Desktop App                                          │
│  ┌──────────────┐     ┌──────────────┐                      │
│  │  web/ React   │────▶│  nearxd      │ (sidecar process)   │
│  │  frontend     │     │  broker      │                      │
│  └──────┬───────┘     └──────┬───────┘                      │
│         │                    │                               │
│         │  Tauri IPC         │  local socket / named pipe    │
│         ▼                    ▼                               │
│  ┌──────────────────────────────┐                            │
│  │  tauri-workspace/src-tauri/  │                            │
│  │  main.rs (Tauri host)        │                            │
│  └──────────────────────────────┘                            │
└─────────────────────────────────────────────────────────────┘

  Browser Extension                    Native TUI
  ┌────────────┐                      ┌──────────┐
  │ extension/ │──native messaging──▶ │ nearx    │
  │ (MV3)      │   via native-host/   │ (Rust)   │
  └────────────┘                      └──────────┘

  Standalone Web (no Tauri)
  ┌──────────────┐     ┌──────────────┐
  │  web/ React   │────▶│ fastnear.com │ (direct API calls)
  │  frontend     │     │ RPC/API      │
  └──────────────┘     └──────────────┘
        or optionally via:
  ┌──────────────┐
  │ nearx-proxy  │  (self-hosted HTTP proxy with auth injection)
  └──────────────┘
```

### Binaries

| Binary | Source | Purpose |
|--------|--------|---------|
| `nearx` | `src/bin/nearx.rs` | Native terminal UI (ratatui). Feature: `native` |
| `nearxd` | `src/bin/nearxd/` | Local broker daemon. Credentials, signing, token resolution, deep-link parsing. Communicates over local sockets / named pipes |
| `nearx-proxy` | `src/bin/nearx-proxy.rs` | Optional HTTP proxy for standalone web deployments. Wraps NEAR RPC with auth injection. Feature: `proxy` |

### What `build-sidecar.mjs` does

Builds `nearxd` in release mode and copies it to `tauri-workspace/src-tauri/binaries/nearxd-<target-triple>` (plus `.exe` on Windows). Tauri bundles this binary and auto-spawns it as a sidecar on app startup. The proxy (`nearx-proxy`) is **not** part of the sidecar — it's a separate deployment concern for hosting the web frontend without Tauri.

### Shared core (`nearx` crate)

`src/lib.rs` is the shared Rust library used by all binaries. It contains the explorer data model, RPC utilities, block/transaction types, router, theme, and platform abstractions. All outbound HTTP requests include an `X-Nearx-Client` header for analytics (e.g. `nearx/0.3.0`, `nearxd/0.3.0`, or `nearx-web`).

## Repository Layout

```text
nearx/
├── src/                     # shared Rust core (nearx crate)
│   ├── bin/nearx.rs         # native TUI binary
│   ├── bin/nearxd/          # broker daemon (14 modules)
│   └── bin/nearx-proxy.rs   # HTTP proxy binary
├── tauri-workspace/         # Tauri v2 desktop shell
├── web/                     # Explorer frontend (React + Vite + Tailwind v4)
├── native-host/             # Browser native messaging host
├── extension/               # Browser extension (MV3)
└── archive/legacy-web-dom/  # Archived former WASM web target
```

### Explorer Frontend Upstream

`web/` is a superset of [`fastnear/explorer-frontend`](https://github.com/fastnear/explorer-frontend). Shared files stay in sync; NEARx-only additions (Tauri integration, signing, staking) are tracked in `web/.explorer-upstream.json`. Run `tools/sync-explorer.sh` to check for upstream divergence — see `CLAUDE.md` section 10 for the full sync workflow.

## Build And Run

### Install dependencies

```bash
corepack enable
yarn install
```

Node 20.x expected (`.nvmrc`).

### Tauri desktop (primary workflow)

```bash
node tools/build-sidecar.mjs         # once, or after nearxd Rust changes
cd tauri-workspace && cargo tauri dev # starts Vite + spawns nearxd sidecar
```

### macOS signed QA build (for Touch ID / fingerprint testing)

```bash
yarn build:macos-qa
open tauri-workspace/target/debug/bundle/macos/NEARx.app
```

This produces a non-adhoc signed local app bundle and signs the bundled `nearxd` sidecar as well. It prefers a `Developer ID Application` certificate when installed, and falls back to `Apple Development` for local QA. Use this path for fingerprint-protected Keychain validation; ad-hoc `cargo tauri dev` and `--no-sign` bundles are not sufficient.

### Web explorer (standalone, no Tauri)

```bash
make dev    # dev server at http://127.0.0.1:1420
make web    # production build -> web/dist/
```

### nearxd broker (standalone)

Only needed if you want to run nearxd independently of Tauri (e.g. for the native TUI or debugging):

```bash
make nearxd
```

### nearx-proxy (optional, for self-hosted web)

Runs a backend HTTP server that proxies NEAR RPC calls with automatic auth token injection. Useful for hosting the web explorer without Tauri:

```bash
cargo run --bin nearx-proxy --features proxy
```

Endpoints: `/health`, `POST /rpc`, `/api/latest`, `/api/block/:height`, `/api/blocks?from=N&limit=M`

### Native TUI

```bash
cargo run --bin nearx --features native
```

## Deep Link Contract

Canonical deep links are versioned under `nearx://v1/...` (see `docs/DEEP_LINK_URI_SPEC.md`).

Tauri flow:

1. OS deep links are received in `tauri-workspace/src-tauri/src/main.rs`.
2. Rust canonicalizes links via `nearxd` when available.
3. Frontend receives deep-link events and maps supported routes:
   - `home` -> `/`
   - `tx/<hash>` -> `/tx/:txHash`
   - `block/<id>` -> `/block/:blockId`
   - `account/<id>` -> `/account/:accountId`
   - `staking` -> `/staking` (Tauri only)
4. Unsupported deep links fall back to home.

## Package Management

This repo is standardized on Yarn Berry with workspaces.

- Root package manager: `yarn@4.12.0`
- Workspaces: `web`, `e2e-tests`
- Linker: `node-modules` (`.yarnrc.yml`)
- Node.js: `20.x` expected for local development (`.nvmrc`)

## Tauri Local Setup + Deep-Link Testing

### Why this matters

On macOS, `nearx://...` deep links do not use dev-time runtime registration. They target the installed app bundle selected by Launch Services, which can be stale. If a stale app is installed, deep links may open legacy UI or show WASM/CSP errors.

### macOS deep-link ready flow (main path)

```bash
cd tauri-workspace
cargo tauri build --debug --bundles app --no-sign
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app
mdfind "kMDItemCFBundleIdentifier == 'com.fastnear.nearx'"
open 'nearx://v1/home'
```

### Linux and Windows deep-link dev flow

Runtime deep-link registration (`register_all`) is used for dev mode.

```bash
xdg-open 'nearx://v1/home'    # Linux
start nearx://v1/home          # Windows
```

## E2E

Tauri WebDriver E2E (`tauri-driver`) is supported on Linux/Windows only.

```bash
yarn workspace nearx-e2e test
```

On macOS, run this suite in Linux CI/container.

## Troubleshooting

### Deep links open stale or wrong app on macOS

```bash
cd tauri-workspace
cargo tauri build --debug --bundles app --no-sign
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app
open 'nearx://v1/home'
```

If the app still looks stale, confirm `/Applications/NEARx.app` is the one registered by `mdfind`.

### Fingerprint QA on macOS

Do not use the `--no-sign` deep-link install path for biometric signing validation. Use the signed QA bundle instead:

```bash
yarn build:macos-qa
open tauri-workspace/target/debug/bundle/macos/NEARx.app
```

If you want Launch Services to target that bundle, copy it into `/Applications` after the signed QA build and re-register it with `lsregister`.

### Error: Cannot find module @rollup/rollup-darwin-arm64

Dependencies were installed in a different platform environment. Run `make repair-js-deps` and retry.

### Error: You have not agreed to the Xcode license agreements

```bash
sudo xcodebuild -license accept
sudo xcodebuild -runFirstLaunch
```

## Canonical Docs

- `CLAUDE.md` -- engineering continuity reference (architecture, contracts, sync workflow)
- `QUICK_START.md`
- `BUILD_VERIFICATION.md`
- `docs/DEEP_LINK_URI_SPEC.md`
- `docs/DEEP_LINKS.md`
- `docs/NEARXD.md`
- `EXTENSION_SETUP.md`
