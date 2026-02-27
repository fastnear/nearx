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

## Compatibility Notes

- Legacy `near://...` inputs are compatibility input only.
- New links and emitted links should use `nearx://v1/...`.

## Quick Manual Validation

### macOS

```bash
open 'nearx://v1/tx/<txHash>'
open 'nearx://v1/block/178923456'
open 'nearx://v1/account/intents.near'
```

### Linux

```bash
xdg-open 'nearx://v1/tx/<txHash>'
```

### Windows

```powershell
start nearx://v1/tx/<txHash>
```

## Documentation Continuity Rule

When deep-link behavior changes:

1. Update `docs/DEEP_LINK_URI_SPEC.md` first.
2. Update this file with runtime behavior and known gaps.
3. Update `README.md`/`QUICK_START.md` examples when user-facing behavior changes.
