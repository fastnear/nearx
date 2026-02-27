# NEARx Quick Start

Last Updated: 2026-02-27

## Prerequisites

```bash
rustup toolchain install 1.89.0
cargo install --locked tauri-cli
```

Node 20.x expected for this repo (`.nvmrc`).

## Install Dependencies

```bash
cd /Users/mikepurvis/near/fn/nearx
corepack enable
yarn install
```

## Build

```bash
# 1) Native messaging host
cd native-host
cargo build --release

# 2) Explorer frontend
cd ..
make web

# 3) Tauri desktop app
cd tauri-workspace
cargo tauri build
```

## Run (Dev)

```bash
# Terminal 1: nearxd broker
cd /Users/mikepurvis/near/fn/nearx
make nearxd

# Terminal 2: Tauri desktop (starts Vite via tauri.conf.json beforeDevCommand)
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri dev
```

If Tauri dev fails with `Cannot find module @rollup/rollup-darwin-arm64`, run `make repair-js-deps` from repo root and retry.

Optional: run standalone web dev server

```bash
cd /Users/mikepurvis/near/fn/nearx
make dev
```

## E2E

```bash
cd /Users/mikepurvis/near/fn/nearx
yarn workspace nearx-e2e test
```

## Deep Link Smoke

```bash
# macOS
open 'nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b'

# Linux
xdg-open 'nearx://v1/block/178923456'

# Windows
start nearx://v1/account/intents.near
```

## Docs

- [README.md](/Users/mikepurvis/near/fn/nearx/README.md)
- [BUILD_VERIFICATION.md](/Users/mikepurvis/near/fn/nearx/BUILD_VERIFICATION.md)
- [DEEP_LINK_URI_SPEC.md](/Users/mikepurvis/near/fn/nearx/docs/DEEP_LINK_URI_SPEC.md)
- [NEARXD.md](/Users/mikepurvis/near/fn/nearx/docs/NEARXD.md)
