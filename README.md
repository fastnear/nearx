# NEARx

NEARx is a NEAR explorer with a shared Rust core (`nearx`), a local broker daemon (`nearxd`), and a React/Vite frontend used by both the web target and Tauri desktop app.

Status date: 2026-02-27

## Active Targets

- Native terminal UI (`nearx`)
- Desktop app (Tauri v2 + `web/dist`)
- Explorer website (`web/` React/Vite app)
- Browser extension + native messaging host

Legacy WASM/DOM web implementation was archived under `archive/legacy-web-dom/`.

## Quick Start (Tauri Desktop)

```bash
# 1. Install JS dependencies (first time / after pulling)
yarn install

# 2. Build the nearxd sidecar (first time / after Rust changes to nearxd)
bash tools/build-sidecar.sh

# 3. Run the desktop app
cd tauri-workspace
cargo tauri dev
```

That's it. `cargo tauri dev` starts the Vite dev server automatically (`beforeDevCommand`) and spawns `nearxd` as a managed sidecar process — no separate terminals needed. The sidecar handles credentials, signing, deep-link parsing, and token resolution. If a standalone `nearxd` is already running on the default socket, the sidecar spawn is skipped and the existing instance is reused.

## Repository Layout

```text
nearx/
├── src/                     # shared Rust core + binaries (nearx, nearxd)
├── tauri-workspace/         # Tauri v2 host
├── web/                     # Explorer frontend (React + Vite)
├── native-host/             # Native messaging host
├── extension/               # Browser extensions
└── archive/legacy-web-dom/  # Archived former WASM web target
```

### Explorer Frontend Upstream

`web/` is a superset of [`fastnear/explorer-frontend`](https://github.com/fastnear/explorer-frontend). Shared files stay in sync; NEARx-only additions (Tauri integration, signing, staking) are tracked in `web/.explorer-upstream.json`. Run `tools/sync-explorer.sh` to check for upstream divergence — see `CLAUDE.md` section 10 for the full sync workflow.

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

## Build And Run

### Install dependencies

```bash
cd /Users/mikepurvis/near/fn/nearx
source "$HOME/.nvm/nvm.sh"
nvm use 20
corepack enable
yarn install
```

### Web explorer

```bash
# Dev server
make dev

# Production build
make web
```

### Tauri desktop

```bash
# Build sidecar (once, or after nearxd Rust changes)
bash tools/build-sidecar.sh

# Run desktop app (starts Vite + spawns nearxd sidecar automatically)
cd tauri-workspace
cargo tauri dev
```

`cargo tauri dev` automatically starts the explorer frontend dev server via Tauri's `beforeDevCommand` and spawns `nearxd` as a sidecar, so you do not need separate terminals for the dev server or the broker daemon.

### nearxd broker (standalone)

Only needed if you want to run nearxd independently of Tauri (e.g. for the native TUI or debugging):

```bash
make nearxd
```

## Tauri Local Setup + Deep-Link Testing

### Why this matters

On macOS, `nearx://...` deep links do not use dev-time runtime registration. They target the installed app bundle selected by Launch Services, which can be stale. If a stale app is installed, deep links may open legacy UI or show WASM/CSP errors.

### Deterministic local desktop flow

```bash
# Build sidecar once, then run Tauri (single terminal)
bash tools/build-sidecar.sh
cd tauri-workspace
cargo tauri dev
```

The Tauri app auto-spawns `nearxd` as a sidecar on startup. If a standalone `nearxd` is already running on the default socket, the app reuses it instead.

### macOS deep-link ready flow (main path)

```bash
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace

# 1) Build a fresh app bundle
cargo tauri build --debug --bundles app --no-sign

# 2) Replace installed app with fresh bundle
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app

# 3) Refresh Launch Services registration
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app

# 4) Verify registration path
mdfind "kMDItemCFBundleIdentifier == 'com.fastnear.nearx'"

# 5) Smoke deep links
open 'nearx://v1/home'
open 'nearx://v1/block/178923456'
open 'nearx://v1/account/intents.near'
```

### Linux and Windows deep-link dev flow

This repo uses runtime deep-link registration (`register_all`) for Linux and Windows dev mode.

```bash
# Linux
xdg-open 'nearx://v1/home'

# Windows
start nearx://v1/home
```

### Confirm you are on current build (macOS)

```bash
mdfind "kMDItemCFBundleIdentifier == 'com.fastnear.nearx'"
strings /Applications/NEARx.app/Contents/MacOS/nearx-tauri | rg 'NEARx'
```

Expected visual sanity check: window title shows `NEARx`.

## E2E

Tauri WebDriver E2E (`tauri-driver`) is supported on Linux/Windows only.

```bash
# Linux or Windows host (or Linux CI/container)
yarn workspace nearx-e2e test
```

On macOS, run this suite in Linux CI/container.

## Troubleshooting

### Deep links open stale or wrong app on macOS

```bash
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri build --debug --bundles app --no-sign
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app
open 'nearx://v1/home'
```

If the app still looks stale, confirm `/Applications/NEARx.app` is the one registered by `mdfind`.

### Error: Cannot find module @rollup/rollup-darwin-arm64

This usually means dependencies were installed in a different platform environment (for example Linux container) and optional platform-native Rollup packages in `node_modules` no longer match macOS.

Recovery sequence:

```bash
cd /Users/mikepurvis/near/fn/nearx
node -e "const fs=require('fs'); for (const p of ['node_modules','web/node_modules','e2e-tests/node_modules','.yarn/install-state.gz']) fs.rmSync(p,{recursive:true,force:true});"
source "$HOME/.nvm/nvm.sh"
nvm use 20
corepack enable
yarn install
ls node_modules/@rollup/rollup-darwin-arm64
cd tauri-workspace
cargo tauri dev
```

Shortcut: `make repair-js-deps` from repo root performs the cleanup + reinstall portion.

### Error: You have not agreed to the Xcode license agreements

If `cargo tauri dev` fails while compiling macOS crates (for example `objc2-exception-helper`), you may see logs like:

- `You have not agreed to the Xcode license agreements`
- `cc-rs ... exit status: 69`
- `failed to run custom build command for objc2-exception-helper`

Recovery sequence:

```bash
sudo xcodebuild -license accept
sudo xcodebuild -runFirstLaunch
xcodebuild -checkFirstLaunchStatus  # optional verification; should return exit code 0

cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri dev
```

## Canonical Docs

- `QUICK_START.md`
- `BUILD_VERIFICATION.md`
- `docs/DEEP_LINK_URI_SPEC.md`
- `docs/DEEP_LINKS.md`
- `docs/NEARXD.md`
- `EXTENSION_SETUP.md`
