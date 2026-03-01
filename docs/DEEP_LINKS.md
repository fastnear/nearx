# NEARx Deep Links

Status: Active
Last Updated: 2026-02-27

This document is the operational companion to the strict URI contract in:

- `docs/DEEP_LINK_URI_SPEC.md`

If there is any conflict, `docs/DEEP_LINK_URI_SPEC.md` is authoritative.

## Canonical URI Contract

NEARx deep links use the `nearx://` scheme and versioned routes:

- `nearx://v1/home`
- `nearx://v1/tx/<txHash>`
- `nearx://v1/block/<blockRef>`
- `nearx://v1/account/<accountId>`
- `nearx://v1/contract/<accountId>`
- `nearx://v1/access-key/<accountId>/<publicKey>`

## Current Runtime Behavior

### Native TUI

- Deep links are accepted from CLI args in `src/bin/nearx.rs`.
- Routing is parsed via `nearx::router::parse(...)` and applied with `app.apply_route(...)`.

### Tauri Desktop

- Tauri registers scheme `nearx` in `tauri-workspace/src-tauri/tauri.conf.json`.
- Runtime ingestion happens in `tauri-workspace/src-tauri/src/main.rs` via:
  - `tauri_plugin_deep_link::init()`
  - single-instance argv forwarding
  - broker-first canonicalization using `nearxd.parse_deep_link`
- Canonical payloads are emitted as `nearx://open` events.

### Explorer Frontend Bridge

- Frontend subscribes through `@tauri-apps/plugin-deep-link` (`web/src/tauri/runtime.ts`).
- Route mapping is centralized in `web/src/tauri/deeplink.ts`.
- Supported mappings:
  - `nearx://v1/home` -> `/`
  - `nearx://v1/tx/<txHash>` -> `/tx/:txHash`
  - `nearx://v1/block/<blockRef>` -> `/block/:blockId`
  - `nearx://v1/account/<accountId>` -> `/account/:accountId`
- Unsupported links are intentionally routed to home.

## Local Testing By Platform

### macOS (installed bundle required)

On macOS, deep-link runtime registration is not available in normal `cargo tauri dev` flow. Test deep links with an installed bundle in `/Applications`.

```bash
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri build --debug --bundles app --no-sign
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app
mdfind "kMDItemCFBundleIdentifier == 'com.fastnear.nearx'"
```

Then validate:

```bash
open 'nearx://v1/home'
open 'nearx://v1/tx/<txHash>'
open 'nearx://v1/block/178923456'
open 'nearx://v1/account/intents.near'
```

### Linux and Windows (dev runtime registration)

This repo calls `register_all` for Linux and Windows dev mode, so `cargo tauri dev` can be used directly.

```bash
# Linux
xdg-open 'nearx://v1/home'

# Windows
start nearx://v1/home
```

## Stale Registration Recovery (macOS)

If deep links open an old app build/UI:

```bash
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri build --debug --bundles app --no-sign
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app
open 'nearx://v1/home'
```

Optional diagnosis:

```bash
strings /Applications/NEARx.app/Contents/MacOS/nearx-tauri | rg 'NEAR Rocks Explorer'
```

## Compatibility Notes

- Legacy `near://...` inputs are compatibility input only.
- New links and emitted links should use `nearx://v1/...`.

## Documentation Continuity Rule

When deep-link behavior changes:

1. Update `docs/DEEP_LINK_URI_SPEC.md` first.
2. Update this file with runtime behavior and known gaps.
3. Update `README.md` and `QUICK_START.md` examples when user-facing behavior changes.
