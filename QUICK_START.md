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
source "$HOME/.nvm/nvm.sh"
nvm use 20
corepack enable
yarn install
```

## Run Desktop Locally

```bash
# Terminal 1: nearxd broker
cd /Users/mikepurvis/near/fn/nearx
make nearxd

# Terminal 2: Tauri desktop (starts Vite via tauri.conf.json beforeDevCommand)
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri dev
```

If Tauri dev fails with `Cannot find module @rollup/rollup-darwin-arm64`, run `make repair-js-deps` from repo root and retry.

## Deep-Link Ready On macOS

`cargo tauri dev` is not enough for macOS deep-link registration. Install a fresh bundled app and refresh Launch Services:

```bash
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri build --debug --bundles app --no-sign
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app
mdfind "kMDItemCFBundleIdentifier == 'com.fastnear.nearx'"
```

Smoke test:

```bash
open 'nearx://v1/home'
open 'nearx://v1/block/178923456'
open 'nearx://v1/account/intents.near'
```

Stale-registration symptom: deep link opens an old app title/UI (for example old title text or WASM CSP errors).

## Deep-Link Smoke (Linux/Windows)

```bash
# Linux
xdg-open 'nearx://v1/home'

# Windows
start nearx://v1/home
```

Linux and Windows dev mode use runtime deep-link registration via Tauri `register_all`.

## Build (Optional)

```bash
# 1) Native messaging host
cd /Users/mikepurvis/near/fn/nearx/native-host
cargo build --release

# 2) Explorer frontend
cd /Users/mikepurvis/near/fn/nearx
make web

# 3) Tauri desktop app
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri build
```

## E2E

```bash
cd /Users/mikepurvis/near/fn/nearx
yarn workspace nearx-e2e test
```

## Docs

- [README.md](/Users/mikepurvis/near/fn/nearx/README.md)
- [BUILD_VERIFICATION.md](/Users/mikepurvis/near/fn/nearx/BUILD_VERIFICATION.md)
- [DEEP_LINK_URI_SPEC.md](/Users/mikepurvis/near/fn/nearx/docs/DEEP_LINK_URI_SPEC.md)
- [DEEP_LINKS.md](/Users/mikepurvis/near/fn/nearx/docs/DEEP_LINKS.md)
- [NEARXD.md](/Users/mikepurvis/near/fn/nearx/docs/NEARXD.md)
