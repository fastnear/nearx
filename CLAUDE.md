# NEARx Engineering Continuity

Status date: 2026-02-26

This file is a technical continuity reference for maintainers and collaborators. It captures current architecture, runtime contracts, and integration boundaries across all targets.

## 1. Canonical Documentation Set

Primary, actively maintained docs:

- `README.md`
- `QUICK_START.md`
- `docs/DEEP_LINK_URI_SPEC.md`
- `docs/DEEP_LINKS.md`
- `docs/NEARXD.md`
- `EXTENSION_SETUP.md`
- `e2e-tests/README.md`

If behavior changes, update these files in the same change set.

## 2. Repository Map

Key directories:

- `src/` - shared core crate (`nearx`)
- `src/bin/nearx.rs` - native TUI entrypoint
- `src/bin/nearx-web-dom.rs` - WASM/DOM entrypoint
- `src/bin/nearxd.rs` - local broker daemon
- `native-host/` - browser native messaging host (stdio length-prefixed JSON)
- `extension/` - browser extension (MV3)
- `tauri-workspace/src-tauri/` - desktop shell and IPC commands
- `web/` - DOM frontend + worker runtime
- `e2e-tests/` - Selenium + tauri-driver E2E suite

## 3. Build Targets And Shared Core

NEARx is quad-target with one Rust core.

### 3.1 Native TUI

- Binary: `nearx`
- Entry: `src/bin/nearx.rs`
- Features: `native`

### 3.2 Web/WASM DOM

- Binary: `nearx-web-dom`
- Entry: `src/bin/nearx-web-dom.rs`
- Features: `dom-web`
- Runtime pattern: worker-hosted WASM, messagepack snapshots/actions

### 3.3 Tauri Desktop

- Shell: `tauri-workspace/src-tauri/src/main.rs`
- Webview frontend: `web/`
- Deep link scheme registration: `nearx`

### 3.4 Extension + Native Host

- Extension background: `extension/background.js`
- Native host: `native-host/src/main.rs`
- Host manifest template: `native-host/com.nearx.native.json`

### 3.5 Terminal Look Parity Contract (TUI <-> Web/Tauri)

Visual parity is intentional and should be treated as a contract:

- source-of-truth palette and semantics: `src/theme.rs::Theme`
- web/tauri consume theme via CSS variables exported by `Theme::to_css_vars()`
- injection happens in `src/bin/nearx-web-dom.rs` startup (`apply_theme_to_dom`)
- shared interaction model comes from `UiSnapshot` / `UiAction` (`src/ui_snapshot.rs`)

Required invariants:

- pane focus uses yellow accent top border (`--accent-strong`)
- square-corner pane geometry and minimal spacing (terminal/DOS aesthetic)
- selected rows include chevron prefix (`›`)
- details pane uses monospaced rendering and windowed payload behavior

## 4. Deep-Link Contract

Strict URI contract is defined in:

- `docs/DEEP_LINK_URI_SPEC.md`

Canonical form:

- `nearx://v1/...`

Canonical routes:

- `home`
- `tx`
- `block`
- `account`
- `contract`
- `access-key`

Compatibility aliases and legacy `near://` are accepted only on input boundaries and normalized.

Shared parser and canonicalizer:

- `src/router.rs`
- `parse_deep_link(raw) -> ParsedDeepLink { route, canonical_uri }`
- `parse(raw) -> Option<Route>` for UI/hash convenience

## 5. App Route Semantics

Route application is centralized in:

- `src/app.rs::apply_route`

Current mapping:

- `tx` route: transactions pane, `hash:<txHash>` filter, optional block hint lock
- `block` route: blocks pane, clears filter, locks to block (height/hash resolution)
- `account` route: transactions pane, `acct:<accountId>` filter
- `contract` route: transactions pane, `receiver:<account> action:FunctionCall,DeployContract` plus optional `method:`
- `access-key` route: transactions pane, `acct:<account> action:AddKey,DeleteKey raw:<publicKey>`
- `home` route: clear filter + return to auto-follow

## 6. nearxd Broker

File: `src/bin/nearxd.rs`
Transport: newline-delimited JSON over unix socket.
Default socket path: `${TMPDIR:-/tmp}/nearxd.sock` (override `NEARXD_SOCKET_PATH`).

### 6.1 Method Surface

Implemented methods:

- `ping`
- `get_runtime_config`
- `parse_deep_link`
- `resolve_fastnear_auth_token` (`get_fastnear_auth_token` alias)
- `resolve_fastnear_api_key` (`get_fastnear_api_key` alias)
- `set_fastnear_auth_token` (`set_fastnear_api_key` alias)
- `clear_fastnear_auth_token` (`clear_fastnear_api_key` alias)
- `open_deep_link`
- `probe_user_presence`
- `request_user_presence`
- `get_signing_settings`
- `set_signing_settings`
- `import_near_credentials`
- `get_near_credential`
- `create_sign_intent`
- `approve_sign_intent`
- `consume_sign_intent`

### 6.2 Token Resolution And Persistence

Resolution precedence:

1. in-memory session token
2. persisted token store backend
3. `FASTNEAR_API_KEY` env (canonical)
4. `FASTNEAR_AUTH_TOKEN` env (legacy alias)

Request auth format:

- append key as `?apiKey=<KEY>` on FastNear RPC/API URLs

Backend selection (`NEARXD_TOKEN_BACKEND`):

- `auto` (default)
  - macOS: keychain primary + file fallback
  - non-macOS: file
- `keychain` (macOS only)
- `file`

File backend path:

- `NEARXD_TOKEN_FILE` or `~/.nearx/fastnear_auth_token`

macOS keychain identity:

- service: `nearxd.fastnear.auth`
- account: `fastnear_auth_token`

Additional keychain services:

- `nearxd.near.credentials` (account namespace: `<network>:<account_id>`)
- `nearxd.signing.settings` (account: `default`)

User-presence adapter env:

- `NEARXD_USER_PRESENCE_ADAPTER=auto|swift|mock|none`
- `auto` defaults to Swift `LocalAuthentication` on macOS and unavailable elsewhere

### 6.3 Signed Intent Lifecycle

Intent methods are navigation-independent and intended for future signing UX.

1. `create_sign_intent`
- Input: `account_id`, `payload`, optional `origin`, optional `expires_in_ms`, optional `require_user_presence`, optional `user_presence_reason`
- Output: `intent_id`, `challenge`, `created_at_ms`, `expires_at_ms`, `status=pending`

2. `approve_sign_intent`
- Input: `intent_id`, `challenge`
- Output: approved state (`status=approved`)

3. `consume_sign_intent`
- Input: `intent_id`, `challenge`
- Output: one-time payload retrieval (`status=consumed`)

Guardrails:

- challenge-bound authorization
- TTL (default 2 minutes, max 10 minutes)
- single-use consume semantics

### 6.4 Credential Import And Settings

`import_near_credentials` supports importing `~/.near-credentials/<network>` JSON credentials, optionally requiring user presence and persisting into keychain.

`get_signing_settings` / `set_signing_settings` manage signing-related settings, with keychain-preferred persistence and file fallback (`~/.nearx/signing_settings.json`).

Credential keychain account namespace:

- `<network>:<account_id>` under service `nearxd.near.credentials`

## 7. Native Host Protocol

File: `native-host/src/main.rs`
Transport: length-prefixed JSON over stdin/stdout.

Inbound messages (`type`):

- `hello`
- `ping`
- `open_deep_link`
- `open_session`
- `create_sign_intent`
- `approve_sign_intent`
- `consume_sign_intent`

Outbound messages (`type`):

- `hello`
- `pong`
- `ok`
- `data`
- `err`

Behavior:

- `open_deep_link`: broker-first (`nearxd`), fallback to direct OS opener only when broker is unavailable
- Sign-intent methods: forwarded to broker and returned as `data` payloads
- `create_sign_intent` forwarding supports optional `require_user_presence` and `user_presence_reason`

Legacy exception (explicit):

- `open_session` currently emits `near://open/session/<id>?readOnly=<0|1>` directly.
- This route is not part of the strict `nearx://v1/...` deep-link contract and should be treated as migration debt.

## 8. Tauri Runtime Contract

File: `tauri-workspace/src-tauri/src/main.rs`

Commands:

- `open_external`
- `get_runtime_config`
- E2E-only commands from `test_api.rs` (feature `e2e`)

Deep-link pipeline:

- On deep-link/open-url/single-instance argv -> `canonicalize_deep_link`
- Uses `nearxd.parse_deep_link` when broker is reachable
- Fallback path accepts strict `nearx://v1/...` and `near://v1/...` remap only
- Emits canonical URL on event `nearx://open`

Runtime config pipeline:

- `get_runtime_config` asks broker with `include_token=true`
- Falls back to env defaults if broker unavailable

## 9. Web + Worker Contract

Files:

- `web/app.js`
- `web/worker.js`
- `src/bin/nearx-web-dom.rs`

Behavior:

- main thread loads runtime config via Tauri command
- sends `runtimeConfig` in worker `init`
- worker sets `self.__NEARX_RUNTIME_CONFIG`
- WASM app reads runtime overrides at startup
- deep-link events (`nearx://open`) are forwarded to worker as `deepLink`
- worker calls `WasmApp.applyDeepLink` and returns updated snapshot

Message protocol:

- main -> worker: `init`, `snapshot`, `action`, `deepLink`, `setDetailsViewport`, `getClipboard`
- worker -> main: `ready`, `snapshot`, `clipboard`, `error`

Performance invariants:

- WASM execution stays off main thread (worker-hosted)
- snapshots/actions use MessagePack + transferables for low overhead
- polling/render loop is intentionally throttled (10 Hz) to avoid unnecessary UI churn

## 10. Extension Integration Model

Files:

- `extension/background.js`
- `extension/content.js`
- `EXTENSION_SETUP.md`

Flow:

1. content script emits `open_deeplink` with canonical `nearx://v1/...`
2. background forwards via `connectNative("com.nearx.native")`
3. native host sends broker request
4. broker validates/canonicalizes and performs operation

Manifest layering:

- browser extension manifest (`extension/manifest*.json`)
- native host manifest (`native-host/com.nearx.native.json`)
- tauri manifest (`tauri-workspace/src-tauri/tauri.conf.json`)

## 11. E2E Test Surface

Files:

- `e2e-tests/test/smoke.spec.mjs`
- `tauri-workspace/src-tauri/src/test_api.rs`

Test IPC commands:

- `nearx_test_emit_deeplink`
- `nearx_test_roundtrip_deeplink`
- `nearx_test_get_last_route`
- `nearx_test_clear_storage`

Roundtrip assertion added:

- legacy alias input canonicalizes to strict `nearx://v1/...` output

## 12. Operational Commands

Core checks:

```bash
cargo check --bin nearx
cargo test --lib
cargo check --bin nearxd
cargo test --bin nearxd
cargo check --manifest-path native-host/Cargo.toml
cargo test --manifest-path native-host/Cargo.toml
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml --features e2e
```

E2E smoke:

```bash
cd e2e-tests
npm test
```

## 13. Continuity Rules

When changing deep links:

1. update `docs/DEEP_LINK_URI_SPEC.md` first
2. update `docs/DEEP_LINKS.md`
3. update `README.md`, `QUICK_START.md`, and relevant E2E tests

When changing broker APIs:

1. update `docs/NEARXD.md`
2. update native-host forwarding (if applicable)
3. update extension/background behavior (if applicable)
4. update test IPC/E2E coverage

When changing extension/native-host plumbing:

1. update `EXTENSION_SETUP.md`
2. ensure manifest naming consistency (`com.nearx.native`)
3. verify host protocol compatibility

When discovering stale docs or duplicates:

1. remove or archive the stale file in the same change set
2. ensure canonical docs remain: `README.md`, `CLAUDE.md`, `docs/*`, `EXTENSION_SETUP.md`, `e2e-tests/README.md`
3. do not leave contradictory guidance in unreferenced scratch docs
