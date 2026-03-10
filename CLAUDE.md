# NEARx Engineering Continuity

Status date: 2026-03-09

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
- `nearx-broker-ipc/` - shared workspace crate: cross-platform `BrokerEndpoint` abstraction over `interprocess` local sockets (used by Tauri, native-host, and nearxd)
- `nearx-plugin-core/` - plugin infrastructure crate (excluded from workspace; types, IPC, registry, traits)
- `native-host/` - browser native messaging host (stdio length-prefixed JSON)
- `extension/` - browser extension (MV3)
- `tauri-workspace/src-tauri/` - desktop shell and IPC commands
- `web/` - React/TypeScript frontend (synced from `fastnear/explorer-frontend`)
- `e2e-tests/` - Playwright + Selenium E2E suite
- `tools/` - build and release scripts (see section 13)

Key config files:

- `rust-toolchain.toml` - Rust 1.89.0, rustfmt + clippy
- `.cargo/config.toml` - WASM target hardening
- `Makefile` - top-level build automation (`make help` for targets)
- `.github/workflows/` - CI: `ci.yml`, `e2e.yml`, `goe-ablation.yml`
- `web/.explorer-upstream.json` - upstream sync config (see section 10)

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
- `set_signing_key_label`
- `get_preferences`
- `set_preferences`

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
- near-cli secure credentials: service `near-<network>-<account_id>`, account `<account_id>:<public_key>`

macOS signer protection semantics:

- nearxd tracks indexed `nearxd_keychain_protection` metadata per signing key
- `biometry_current_set` is the only macOS Keychain state NEARx treats as fingerprint-ready for signing
- Keychain import is opt-in only. Signing works from original sources (`legacy_file`, `near_cli_secure`) without requiring import. The frontend does not prompt or warn about keychain import
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
- macOS imports report actual Keychain protection outcomes (`nearxd_keychain_protection` field)

Signer discovery / enforcement:

- `list_near_signing_keys` rows include `nearxd_keychain_protection`
- `reprotect_near_signing_key` repairs an existing nearxd Keychain item in place
- `sign_transaction` silently falls back to the next software source when `credential_source=nearxd_keychain` is requested but the keychain copy is not biometric-protected
- `list_near_signing_keys` rows include `security_level` (`secure`, `hardware`, `basic`) for simplified frontend display

`get_signing_settings` / `set_signing_settings` manage signing-related settings, persisted to `~/.nearx/signing_settings.json`. Settings are non-secret metadata (key labels, preferences, staking watchlist, hardware wallet index). The file backend uses atomic write (temp + rename) with `0o600` permissions on Unix. Settings are file-only — no keychain/OS-secret-store involvement. The wire parameter `prefer_keychain` is accepted but ignored.

**Ledger transaction format:** Hardware wallet signing serializes `TransactionV0` directly (without the `Transaction` enum discriminant prefix) because the NEAR Ledger app expects raw V0 borsh bytes. The `Transaction::V0(...)` wrapper is used only for hash computation and `SignedTransaction` construction.

All settings read-modify-write operations are serialized via `BrokerState::settings_lock` (`Mutex<()>`) to prevent concurrent clobbering.

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

### 6.5 Sign Transaction Validation

`sign_transaction` enforces the NEAR protocol's sender==receiver constraint before signing. Actions classified as self-targeting (DeployContract, Stake, AddKey, DeleteKey, DeleteAccount) are rejected with `ERR_PARAMS` if `receiver_id` differs from `signer_id`. This is defense-in-depth: the frontend hides the Receiver field for these action types, but the broker validates regardless of caller.

This classification comes from nearcore's `check_actor_permissions` (`runtime/runtime/src/actions.rs`). Actions that allow a different receiver: Transfer, FunctionCall, CreateAccount.

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
- `get_preferences`
- `set_preferences`
- `sign_transaction`
- `set_signing_key_label`
- `fetch_fastnear_json`
- `pick_wasm_file` (native file dialog for WASM contract selection, used by DeployContract)
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

`web/CLAUDE.md` documents upstream explorer-frontend architecture (routing, API layer, hooks, widgets, components). This section documents NEARx-specific additions. The `nearx_only` list in `web/.explorer-upstream.json` is the authoritative boundary between upstream-synced and NEARx-only files.

### 9.1 Settings Page

File: `web/src/pages/Settings.tsx`
Route: `/settings` (Tauri-only, not shown in standalone mode)

Consolidates all signer management into one place:

- **Security**: "Always require fingerprint" toggle (`always_prompt_user_presence` preference, enforced broker-side in `sign_transaction`)
- **Signing Keys**: read-only list of discovered keys grouped by account, with label editing and simplified security badges (Secure/Standard/Hardware via `security_level`). Footer links to Staking/Sign pages for Ledger connection.
- **Platform Info**: read-only capabilities display (platform, secure store, biometric support, hardware wallet support, transport, network)

Preferences are stored via `get_preferences` / `set_preferences` broker methods, persisted in signing settings (file-only at `~/.nearx/signing_settings.json`).

Hook: `web/src/hooks/usePreferences.ts` — loads preferences on mount, provides `updatePreference(key, value)` with optimistic update and rollback.

### 9.2 Signer UX Architecture

Signer management (labeling, Ledger connection) is centralized in the Settings page. Staking and Sign Transaction pages use simplified signer controls: account dropdown + key dropdown + action buttons. Credential source resolution is handled entirely by the backend — the frontend never tracks or passes `credential_source` for software keys.

- Shared signer selection state: `web/src/hooks/useSignerSelection.ts`
  - owns selected account, public key, account/key reloads
  - uses `list_near_signing_accounts` + `list_near_signing_keys` through Tauri runtime methods
  - persists last selected account and key via `web/src/hooks/useAccountPrefs.ts`
- Shared signer summary/status: `web/src/lib/signerSummaryStatus.ts`
  - used by both `web/src/pages/Staking.tsx` and `web/src/pages/SignTransaction.tsx`
  - 3-state model: hardware error / error -> neutral -> ready
- Shared quick controls:
  - `web/src/components/SignerQuickSelectors.tsx` (2-column: account + key)
  - `web/src/components/SignerSummaryCard.tsx`
- Key utilities: `web/src/lib/signerSourceSelection.ts` (signingKeyId, keyHasUsableSource, preferSigningKey)

NEARx-only shared libraries in `web/src/lib/` (all excluded from upstream sync):

- `signerSourceSelection.ts` - key identification, usable-source detection, key preference
- `signerSummaryStatus.ts` - 3-state readiness model for signer summary cards
- `broadcastSummary.ts` - broadcast result classification (success/submitted/failed)
- `hardwareWalletDisplay.ts` - display formatting for signing accounts, keys, permissions
- `ledgerConnectionUi.ts` - Ledger connection/error state management, shared by Staking and Sign pages
- `sourceUpgrade.ts` - credential source upgrade path detection

### 9.3 Staking Page Behavior

File: `web/src/pages/Staking.tsx`

- The selected staking account is the canonical account for both delegation loading and signer key loading
- The watchlist is a saved-account convenience list, not a separate source of truth for signer readiness
- Choosing an account from signer controls auto-seeds the watchlist if needed
- Staking actions require a full-access key with a usable local source
- Credential source is auto-resolved by the backend; users manage key labels in Settings
- Action success UI now shows the full transaction hash, not a truncated hash fragment
- Broadcast uses `broadcast_tx_commit` with client-side timeout, falling back to `broadcast_tx_async` on RPC timeout; UI displays 3 states: success (green), submitted/pending (yellow), failed (red)

### 9.4 Sign Transaction Behavior

File: `web/src/pages/SignTransaction.tsx`

- Supports all 8 core NEAR action types: Transfer, FunctionCall, DeployContract, CreateAccount, DeleteAccount, DeleteKey, AddKey (FullAccess + FunctionCall permission), Stake
- Action selector is a `<select>` dropdown; each type renders its own form fields
- Non-FunctionCall actions require a full-access key (enforced client-side in `evaluateKeyCompatibility`)
- Self-targeting actions (DeployContract, Stake, AddKey, DeleteKey, DeleteAccount) hide the Receiver field and auto-set `receiver_id` to the signer account via `receiverIsImplicit`; this mirrors the NEAR protocol's `check_actor_permissions` constraint that requires sender==receiver for these action types
- Non-self-targeting actions (Transfer, FunctionCall, CreateAccount) show the Receiver field independently from signer selection
- Credential source is auto-resolved by the backend; `sign_transaction` calls omit `credential_source` for software keys
- Confirmation modal supports three actions:
  - Cancel
  - Sign
  - Sign + broadcast
- Broadcast results are summarized via `web/src/lib/broadcastSummary.ts`
  - tx hash is shown as a clickable link on success, submission, and failure paths
  - `broadcast_tx_commit` falls back to `broadcast_tx_async` on RPC timeout; `async_submitted` marker yields `success: null` (submitted but execution unknown)
  - raw RPC broadcast payload remains available for inspection when present

### 9.5 Frontend Test Harness

Minimal frontend unit/smoke coverage now exists under `web/`.

- Runner/config:
  - `web/package.json`
  - `web/vite.config.ts`
  - `web/src/test/setup.ts`
- Current targeted tests:
  - `web/src/api/retry.test.ts`
  - `web/src/components/FilterableCombobox.test.tsx`
  - `web/src/hooks/useAccountPrefs.test.tsx`
  - `web/src/lib/broadcastSummary.test.ts`
  - `web/src/lib/signerSourceSelection.test.ts`
  - `web/src/lib/signerSummaryStatus.test.ts`
  - `web/src/lib/sourceUpgrade.test.ts`

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

A top-level `Makefile` provides convenience targets (`make help` for full list). The commands below are the underlying invocations.

### 13.1 Core Checks

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

### 13.2 Web Frontend

```bash
cd web && npx tsc -b          # typecheck only
npm --prefix web run test      # Vitest unit tests
```

### 13.3 Desktop Dev

```bash
node tools/build-sidecar.mjs   # build nearxd sidecar (required for Tauri dev)
cd tauri-workspace && cargo tauri dev   # starts Vite + spawns nearxd sidecar
```

### 13.4 E2E

```bash
cd e2e-tests && npm test
```

### 13.5 Tools Directory

- `tools/build-sidecar.mjs` - copies nearxd binary into Tauri sidecar bundle location
- `tools/build-sidecar.sh` - shell wrapper invoked by `make sidecar`
- `tools/build-macos-qa.sh` - local macOS signed QA build (prefers `Developer ID Application`, falls back to `Apple Development`)
- `tools/sync-explorer.sh` - upstream explorer-frontend sync tool (see section 10)
- `tools/preflight.sh` - pre-release checks
- `tools/alpha_preflight.sh` - alpha release checks
- `tools/release.sh` - release automation
- `tools/list_recent_by_git.sh` - list recently modified files by git history
- `tools/list_recent_created.sh` - list recently created files

### 13.6 Dependency Versions

- Rust toolchain: 1.89.0 (pinned in `rust-toolchain.toml`)
- `near-primitives` 0.27.0 — supports the classic 8 action types + Delegate; newer nearcore types (DeployGlobalContract, UseGlobalContract, TransferToGasKey, WithdrawFromGasKey) are not available at this version
- `near-crypto` 0.27.0
- `interprocess` — local socket transport (used by nearxd, Tauri, native-host via `nearx-broker-ipc`)

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

When adding new NEAR action types:

1. check nearcore `check_actor_permissions` (`runtime/runtime/src/actions.rs`) for the sender==receiver constraint
2. self-targeting actions (sender must equal receiver): DeployContract, Stake, AddKey, DeleteKey, DeleteAccount
3. different-receiver actions: Transfer, FunctionCall, CreateAccount
4. update frontend `receiverIsImplicit` in `web/src/pages/SignTransaction.tsx`
5. update backend validation in `src/bin/nearxd/broker.rs` (self-targeting guard after `parse_near_actions`)
6. update action type docs in `docs/NEARXD.md`
7. update `near-primitives` version note in section 13.6 if upgrading

When discovering stale docs or duplicates:

1. remove or archive the stale file in the same change set
2. ensure canonical docs remain: `README.md`, `CLAUDE.md`, `docs/*`, `EXTENSION_SETUP.md`, `e2e-tests/README.md`
3. do not leave contradictory guidance in unreferenced scratch docs
