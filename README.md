# NEARx

NEARx is a NEAR explorer with a shared Rust core (`nearx`), a local broker daemon (`nearxd`), and a React/Vite frontend used by both the web target and Tauri desktop app.

Status date: 2026-02-27

## Active Targets

- Native terminal UI (`nearx`)
- Desktop app (Tauri v2 + `web/dist`)
- Explorer website (`web/` React/Vite app)
- Browser extension + native messaging host

Legacy WASM/DOM web implementation was archived under `archive/legacy-web-dom/`.

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
cd tauri-workspace
cargo tauri dev
```

`cargo tauri dev` automatically starts the explorer frontend dev server via Tauri's `beforeDevCommand`, so you do not need a separate `make dev` terminal for desktop development.

### nearxd broker

```bash
make nearxd
```

### E2E

Tauri WebDriver E2E (`tauri-driver`) is supported on Linux/Windows only.

```bash
# Linux or Windows host (or Linux CI/container)
yarn workspace nearx-e2e test
```

On macOS, run this suite in Linux CI/container.

## Troubleshooting

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

## Canonical Docs

- `QUICK_START.md`
- `BUILD_VERIFICATION.md`
- `docs/DEEP_LINK_URI_SPEC.md`
- `docs/DEEP_LINKS.md`
- `docs/NEARXD.md`
- `EXTENSION_SETUP.md`
