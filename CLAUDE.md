# NEARx Engineering Continuity

Status date: 2026-03-08

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

Supplementary chapters (detailed reference):

- `md-claude-chapters/01-architecture.md` - codebase structure, Rust core, React frontend, upstream sync
- `md-claude-chapters/02-tauri-and-extension.md` - desktop shell, sidecar, deep links, extension, E2E
- `md-claude-chapters/03-configuration-and-ops.md` - env vars, CLI args, tokens, build commands
- `md-claude-chapters/04-user-guide.md` - keyboard shortcuts, filtering, mouse, fullscreen, accessibility

If behavior changes, update these files in the same change set.

## 2. Repository Map

Key directories:

- `src/` - shared core crate (`nearx`)
- `src/bin/nearx.rs` - native TUI entrypoint
- `src/bin/nearxd/` - local broker daemon (modular: main, broker, socket, token, keychain, credentials, settings, signing, hardware_wallet, user_presence, rpc, config, util, tests)
- `src/bin/nearx-proxy.rs` - backend HTTP proxy server for web frontend
- `native-host/` - browser native messaging host (stdio length-prefixed JSON)
- `extension/` - browser extension (MV3)
- `tauri-workspace/src-tauri/` - desktop shell and IPC commands
- `web/` - React/TypeScript frontend (synced from `fastnear/explorer-frontend`)
- `e2e-tests/` - Playwright + Selenium E2E suite
- `tools/build-macos-qa.sh` - local macOS signed QA build (prefers `Developer ID Application`, falls back to `Apple Development`, verifies app + sidecar are non-adhoc)

## 3. Build Targets And Shared Core

NEARx is quad-target with one Rust core.

### 3.1 Native TUI

- Binary: `nearx`
- Entry: `src/bin/nearx.rs`
- Features: `native`

### 3.2 Web Frontend

- Entry: `web/src/main.tsx` (React 18 / TypeScript / Tailwind v4 / Vite)
- Build: `npm --prefix web run build` (output: `web/dist/`)
- Dev: `npm --prefix web run dev`
- Test: `npm --prefix web run test`
- Synced from `fastnear/explorer-frontend` with NEARx additions (see section 10)
- Tauri integration: `web/src/tauri/runtime.ts`, `web/src/tauri/deeplink.ts`

### 3.3 Tauri Desktop

- Shell: `tauri-workspace/src-tauri/src/main.rs`
- Webview frontend: `web/`
- Deep link scheme registration: `nearx`
- Sidecar: auto-spawns `nearxd` if no standalone instance is running (see section 8)
- Sidecar build: `node tools/build-sidecar.mjs` (copies binary to `tauri-workspace/src-tauri/binaries/`, including `.exe` on Windows)
- Local signed QA build: `yarn build:macos-qa` (macOS only)

### 3.4 Extension + Native Host

- Extension background: `extension/background.js`
- Native host: `native-host/src/main.rs`
- Host manifest template: `native-host/com.nearx.native.json`

### 3.5 Backend Proxy

- Binary: `nearx-proxy`
- Entry: `src/bin/nearx-proxy.rs`
- Features: `proxy`
- Lightweight HTTP API wrapping NEAR RPC calls for the web frontend
- Endpoints: `/health`, `/rpc` (JSON-RPC proxy), `/api/latest`, `/api/block/:height`, `/api/blocks`

### 3.6 Visual Design Notes

The native TUI (`nearx`) uses `src/theme.rs::Theme` for its palette and rendering semantics.

The web/Tauri frontend uses its own CSS (Tailwind v4) and is not driven by `Theme::to_css_vars()`. Visual parity between TUI and web is no longer a strict contract -- the web frontend follows upstream `explorer-frontend` design conventions.

`Theme::to_css_vars()` still exists in `src/theme.rs` but is unused by the current web frontend (it was used by the archived WASM/DOM build).

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
- `staking`

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
- `staking` route: staking dashboard (Tauri/web only, ignored in TUI), optional `?account=<accountId>`

## 6. nearxd Broker

Directory: `src/bin/nearxd/` (entry: `main.rs`)
Transport: newline-delimited JSON over local sockets via `interprocess`.
Default endpoint:

- macOS/Linux: filesystem socket at `${TMPDIR:-/tmp}/nearxd.sock`
- Windows: namespaced local endpoint `name:nearxd`

Endpoint env vars:

- `NEARXD_ENDPOINT` (canonical, cross-platform)
- `NEARXD_SOCKET_PATH` (legacy Unix compatibility alias)

### 6.1 Method Surface

Implemented methods:

- `ping`
- `get_runtime_config`
- `get_signing_capabilities`
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
- `list_staking_watchlist`
- `add_staking_watchlist_account`
- `remove_staking_watchlist_account`
- `connect_hardware_wallet`
- `list_near_signing_accounts`
- `list_near_signing_keys`
- `import_near_signing_keys`
- `reprotect_near_signing_key`
- `list_near_credentials`
- `import_near_credentials`
- `get_near_credential`
- `create_sign_intent`
- `approve_sign_intent`
- `consume_sign_intent`
- `sign_transaction`

### 6.2 Token Resolution And Persistence

Resolution precedence:

1. in-memory session token
2. persisted token store backend
3. `FASTNEAR_API_KEY` env (canonical)
4. `FASTNEAR_AUTH_TOKEN` env (legacy alias)

Request auth format:

- append key as `?apiKey=<KEY>` on FastNear RPC/API URLs

Client identification:

- All outbound HTTP requests include `X-Nearx-Client` header
- Rust TUI: `nearx/<version>` (via shared `reqwest::Client` in `src/rpc_utils.rs`)
- Rust daemon: `nearxd/<version>` (in `src/bin/nearxd/rpc.rs`)
- Rust proxy: `nearx/<version>` (in `src/bin/nearx-proxy.rs`)
- Web frontend: `nearx-web` (via `nearxHeaders` in `web/src/config.ts`)

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

Additional secure-store services:

- `nearxd.near.credentials` (scoped account namespace: `<network>:<account_id>:<public_key>`, with legacy fallback `<network>:<account_id>`)
- `nearxd.signing.settings` (account: `default`)
- near-cli secure credentials: service `near-<network>-<account_id>`, account `<account_id>:<public_key>`

macOS signer protection semantics:

- nearxd tracks indexed `nearxd_keychain_protection` metadata per signing key
- `biometry_current_set` is the only macOS Keychain state NEARx treats as fingerprint-ready for signing
- legacy or fallback Keychain items surface as `unknown`, `unprotected`, or `user_presence` and require explicit import/repair before `credential_source=nearxd_keychain` is allowed
- Keychain import is additive only; original `legacy_file` and `near_cli_secure` sources remain available
- ad-hoc debug Tauri builds may fall back to weaker local sources because protected Keychain writes are not reliable without a properly signed app bundle and signed `nearxd` sidecar
- local biometric QA can use an `Apple Development`-signed bundle; release/distribution still requires `Developer ID Application` and notarization

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

`import_near_credentials` and `import_near_signing_keys` support importing legacy `~/.near-credentials/<network>` JSON credentials and near-cli secure credentials from the OS secure store.

Secure-store behavior:

- wire field `persist_in_keychain` is retained for compatibility, but now means "persist in OS secure storage"
- macOS uses Keychain
- Linux uses Secret Service/libsecret
- Windows uses Credential Manager
- if secure storage is unavailable, the broker reports capability failure instead of silently storing plaintext credentials

User-presence behavior:

- `require_user_presence` is only enforced when the active adapter reports support
- in practice this is macOS `LocalAuthentication` or the mock adapter used in tests
- Linux/Windows imports and reads rely on OS secure-store protections without a separate biometric prompt
- macOS imports now report actual Keychain protection outcomes and may surface `nearxd_keychain_import_required=true` when the resulting Keychain copy is not verified biometric

Signer discovery / enforcement:

- `list_near_signing_keys` rows now include `nearxd_keychain_protection` and `nearxd_keychain_import_required`
- `reprotect_near_signing_key` repairs an existing nearxd Keychain item in place
- `sign_transaction` rejects explicit `credential_source=nearxd_keychain` with `ERR_IMPORT_REQUIRED` unless indexed protection is `biometry_current_set`

`get_signing_settings` / `set_signing_settings` manage signing-related settings, with secure-store-preferred persistence and file fallback (`~/.nearx/signing_settings.json`). Returned source strings may still use the historical label `keychain` for compatibility.

`get_signing_capabilities` returns:

- `platform`
- `transport`
- `secure_store_backend`
- `supports_legacy_import`
- `supports_near_cli_secure`
- `supports_secure_store_persistence`
- `supports_user_presence`
- `supports_hardware_wallet_connect`
- `supports_hardware_wallet_sign`

Credential secure-store account namespace:

- `<network>:<account_id>:<public_key>` under service `nearxd.near.credentials`
- legacy fallback `<network>:<account_id>` remains readable for backward compatibility

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

### 8.1 nearxd Sidecar

On startup, the Tauri app checks the configured broker endpoint:

- If a standalone `nearxd` is already listening, it reuses that instance
- Otherwise, it spawns `nearxd` as a managed sidecar via `tauri-plugin-shell`
  - macOS/Linux sidecar endpoint: `$TMPDIR/nearxd-tauri-<pid>.sock`
  - Windows sidecar endpoint: `name:nearxd-tauri-<pid>`
  - `NEARXD_ENDPOINT` env is always pointed at the sidecar endpoint
  - `NEARXD_SOCKET_PATH` is also exported on Unix for compatibility
  - Sidecar is killed on app exit (`SidecarChild` drop)
- Sidecar binary must be pre-built: `node tools/build-sidecar.mjs`
- If the sidecar binary is not bundled, broker-dependent features (credentials, signing, deep-link parsing) are unavailable

### 8.2 Tauri Commands

Broker-backed commands forward to `nearxd` via the local-socket transport:

- `open_external`
- `get_runtime_config`
- `get_signing_capabilities`
- `request_user_presence`
- `list_staking_watchlist`
- `add_staking_watchlist_account`
- `remove_staking_watchlist_account`
- `connect_hardware_wallet`
- `list_near_signing_accounts`
- `list_near_signing_keys`
- `import_near_signing_keys`
- `reprotect_near_signing_key`
- `list_near_credentials`
- `import_near_credentials`
- `sign_transaction`
- E2E-only commands from `test_api.rs` (feature `e2e`)

### 8.3 Deep-Link Pipeline

- On deep-link/open-url/single-instance argv -> `canonicalize_deep_link`
- Uses `nearxd.parse_deep_link` when broker is reachable
- Fallback path accepts strict `nearx://v1/...` and `near://v1/...` remap only
- Emits canonical URL on event `nearx://open`

### 8.4 Runtime Config Pipeline

- `get_runtime_config` asks broker with `include_token=true`
- Falls back to env defaults if broker unavailable

## 9. Web Frontend Contract

The web frontend is a React/TypeScript/Vite app in `web/`.

Files:

- `web/src/main.tsx` - React app entry
- `web/src/tauri/runtime.ts` - Tauri command invocations (`get_runtime_config`, `sign_transaction`, etc.)
- `web/src/tauri/deeplink.ts` - deep-link event listener from Tauri shell
- `web/CLAUDE.md` - detailed frontend architecture reference

Modes:

- **Tauri mode** (`VITE_TAURI=true`): imports `@tauri-apps/api`, invokes commands via `web/src/tauri/runtime.ts`, receives deep-link events
- **Standalone mode**: same app served by Vite, configurable `VITE_API_BASE_URL` for API endpoint

The frontend communicates with `nearxd` only when running inside Tauri (via Tauri commands that forward to the broker). In standalone mode, it talks directly to NEAR RPC endpoints.

### 9.1 Signer UX Architecture

Current signer/account UX for NEARx-only pages is shared rather than page-local.

- Shared signer selection state: `web/src/hooks/useSignerSelection.ts`
  - owns selected account, public key, credential source, account/key reloads, and source fallback resolution
  - uses `list_near_signing_accounts` + `list_near_signing_keys` through Tauri runtime methods
  - persists the last selected credential source per `(page context, account_id, public_key)` via `web/src/hooks/useAccountPrefs.ts`
- Shared signer summary/status priority: `web/src/lib/signerSummaryStatus.ts`
  - used by both `web/src/pages/Staking.tsx` and `web/src/pages/SignTransaction.tsx`
  - normal priority order is: hardware error -> blocking error -> neutral/selection-needed -> source-needed -> incompatible -> advisory -> ready
- Shared Keychain import/repair semantics:
  - `web/src/lib/sourceUpgrade.ts`
  - `web/src/lib/signerSourceSelection.ts`
  - when `nearxd_keychain_import_required=true`, both Staking and Sign Tx block `Keychain` signing and show inline import/repair CTA
  - if the user explicitly switches to `File system` or `OS secrets`, actions remain allowed but the UI warns that fingerprint verification is not used
- Shared quick controls and modal shell:
  - `web/src/components/SignerQuickSelectors.tsx`
  - `web/src/components/SignerSummaryCard.tsx`
  - `web/src/components/ManageSignerPanel.tsx`
  - `web/src/components/LedgerConnectionPanel.tsx`

### 9.2 Staking Page Behavior

File: `web/src/pages/Staking.tsx`

- The selected staking account is the canonical account for both delegation loading and signer key loading
- The watchlist is a saved-account convenience list, not a separate source of truth for signer readiness
- Choosing an account from signer controls auto-seeds the watchlist if needed
- Staking actions require a full-access key with a usable local source
- When `Keychain` is the selected source on macOS, staking actions additionally require verified biometric protection (`nearxd_keychain_import_required=false`)
- Action success UI now shows the full transaction hash, not a truncated hash fragment

### 9.3 Sign Transaction Behavior

File: `web/src/pages/SignTransaction.tsx`

- Receiver remains independent from signer selection except when it is blank and a signer account is first chosen
- Confirmation modal supports three actions:
  - Cancel
  - Sign
  - Sign + broadcast
- Broadcast results are summarized via `web/src/lib/broadcastSummary.ts`
  - tx hash is shown on both success and failure paths
  - raw RPC broadcast payload remains available for inspection when present

### 9.4 Frontend Test Harness

Minimal frontend unit/smoke coverage now exists under `web/`.

- Runner/config:
  - `web/package.json`
  - `web/vite.config.ts`
  - `web/src/test/setup.ts`
- Current targeted tests:
  - `web/src/hooks/useAccountPrefs.test.tsx`
  - `web/src/lib/signerSummaryStatus.test.ts`
  - `web/src/lib/broadcastSummary.test.ts`

This is not a full browser E2E replacement. For signer regressions, keep desktop/manual validation for staking and Sign Tx flows in addition to `npm --prefix web run test`.

## 10. Explorer Frontend Upstream Parity

`web/` is a superset of [`fastnear/explorer-frontend`](https://github.com/fastnear/explorer-frontend). Shared files should stay in sync with upstream; NEARx-only files are excluded from comparison.

Config: `web/.explorer-upstream.json`
Sync tool: `tools/sync-explorer.sh`

### 10.1 Boundary

`nearx_only` in the config lists paths that exist only in NEARx (Tauri integration, signing, staking, sidebar, etc.). Everything else in `web/src/` and `web/public/` is expected to match upstream at the synced commit.

### 10.2 Sync Commands

```bash
tools/sync-explorer.sh              # summary: identical / diverged / missing counts
tools/sync-explorer.sh --latest     # compare against upstream HEAD (not last synced SHA)
tools/sync-explorer.sh --diff       # show full diffs for diverged files
tools/sync-explorer.sh --apply      # interactive per-file overwrite from upstream
tools/sync-explorer.sh --bump       # update synced_sha to upstream HEAD after resolving
```

### 10.3 Sync Workflow

1. `tools/sync-explorer.sh` — reports "Upstream has N new commits since last sync"
2. `--latest --diff` — shows what changed upstream vs local
3. For each diverged file: take upstream change, keep local, or merge manually
4. `--apply` — interactively overwrite files where upstream is preferred
5. `--bump` — record the new synced SHA after all divergences are resolved or intentional

If adding new NEARx-only files under `web/`, add them to the `nearx_only` list in the config so they are excluded from upstream comparison.

## 11. Extension Integration Model

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

## 12. E2E Test Surface

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

## 13. Operational Commands

Core checks:

```bash
cargo check --bin nearx
cargo test --lib
cargo check --bin nearxd
cargo test --bin nearxd
cargo check --bin nearx-proxy --features proxy
cargo check --manifest-path native-host/Cargo.toml
cargo test --manifest-path native-host/Cargo.toml
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml --features e2e
```

Web frontend typecheck:

```bash
cd web && npx tsc -b
```

Web frontend tests:

```bash
npm --prefix web run test
```

Build nearxd sidecar (required for Tauri dev):

```bash
node tools/build-sidecar.mjs
```

Tauri dev (single command — starts Vite + spawns nearxd sidecar):

```bash
cd tauri-workspace && cargo tauri dev
```

E2E smoke:

```bash
cd e2e-tests
npm test
```

## 14. Continuity Rules

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

When syncing explorer upstream:

1. run `tools/sync-explorer.sh --latest --diff` to assess divergence
2. if taking upstream changes, verify NEARx integrations (Tauri hooks, dark mode, Sidebar) still work
3. run `--bump` only after all divergences are resolved or intentional
4. if adding new NEARx-only files under `web/`, add them to `nearx_only` in `web/.explorer-upstream.json`

When discovering stale docs or duplicates:

1. remove or archive the stale file in the same change set
2. ensure canonical docs remain: `README.md`, `CLAUDE.md`, `docs/*`, `EXTENSION_SETUP.md`, `e2e-tests/README.md`
3. do not leave contradictory guidance in unreferenced scratch docs
