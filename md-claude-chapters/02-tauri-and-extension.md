# Chapter 2: Tauri Desktop & Extension

Desktop app shell, sidecar lifecycle, deep-link pipeline, extension/native-host integration, and E2E testing.

## Tauri Desktop App

### Identity

- **Bundle ID**: `com.fastnear.nearx`
- **Deep-link scheme**: `nearx://`
- **Frontend dist**: `../../web/dist` (React/Vite build output)

### Dev Workflow

```bash
# Build nearxd sidecar first (required)
bash tools/build-sidecar.sh

# Start Tauri dev (runs Vite dev server + Tauri shell)
cd tauri-workspace && cargo tauri dev
```

The `beforeDevCommand` in `tauri.conf.json` runs:
```
VITE_TAURI=true yarn --cwd .. workspace explorer-frontend dev --host 127.0.0.1 --port 1420 --strictPort
```

### nearxd Sidecar Lifecycle

On startup, the Tauri app (`tauri-workspace/src-tauri/src/main.rs`):

1. Checks the default nearxd socket path (`${TMPDIR:-/tmp}/nearxd.sock`)
2. If a standalone `nearxd` is already listening, reuses that instance
3. Otherwise, spawns `nearxd` as a managed sidecar via `tauri-plugin-shell`
   - Sidecar socket: `$TMPDIR/nearxd-tauri-<pid>.sock`
   - `NEARXD_SOCKET_PATH` env pointed at the sidecar socket
   - Sidecar killed on app exit (`SidecarChild` drop)

The sidecar binary must be pre-built with `bash tools/build-sidecar.sh`. Without it, broker-dependent features (credentials, signing, deep-link parsing) are unavailable.

### Tauri Commands

All commands forward to `nearxd` via unix socket:

- `open_external` -- open URL in system browser
- `get_runtime_config` -- fetch runtime config (with token)
- `request_user_presence` -- biometric prompt
- `list_near_credentials` -- enumerate imported credentials
- `import_near_credentials` -- import from `~/.near-credentials`
- `sign_transaction` -- sign with imported key
- E2E-only commands from `test_api.rs` (feature `e2e`)

### Deep-Link Pipeline

1. Deep-link/open-url/single-instance argv triggers `canonicalize_deep_link`
2. Uses `nearxd.parse_deep_link` when broker is reachable
3. Fallback path accepts strict `nearx://v1/...` and `near://v1/...` remap only
4. Emits canonical URL on event `nearx://open`
5. Frontend listens via `web/src/tauri/deeplink.ts`

### Runtime Config Pipeline

- `get_runtime_config` asks broker with `include_token=true`
- Falls back to env defaults if broker unavailable

## Browser Extension & Native Host

### Extension (MV3)

Files: `extension/background.js`, `extension/content.js`

Flow:
1. Content script detects transaction pages, emits `open_deeplink` with `nearx://v1/...`
2. Background forwards via `connectNative("com.nearx.native")`
3. Native host sends broker request
4. Broker validates/canonicalizes and performs operation

### Native Host

File: `native-host/src/main.rs`
Transport: length-prefixed JSON over stdin/stdout.

**Inbound messages** (`type`): `hello`, `ping`, `open_deep_link`, `open_session`, `create_sign_intent`, `approve_sign_intent`, `consume_sign_intent`

**Outbound messages** (`type`): `hello`, `pong`, `ok`, `data`, `err`

Behavior:
- `open_deep_link`: broker-first, fallback to direct OS opener
- Sign-intent methods: forwarded to broker, returned as `data` payloads
- `open_session`: legacy exception, emits `near://open/session/<id>?readOnly=<0|1>` directly (migration debt)

### Manifest Layering

- Browser extension: `extension/manifest*.json`
- Native host: `native-host/com.nearx.native.json`
- Tauri: `tauri-workspace/src-tauri/tauri.conf.json`

Setup guide: `EXTENSION_SETUP.md`

## E2E Testing

### Test Stack

Playwright for web target testing. Selenium + tauri-driver for desktop (Linux/Windows).

### Test IPC Commands (feature `e2e`)

From `tauri-workspace/src-tauri/src/test_api.rs`:

- `nearx_test_emit_deeplink` -- inject deep link without OS registration
- `nearx_test_roundtrip_deeplink` -- verify canonicalization round-trip
- `nearx_test_get_last_route` -- query routing state
- `nearx_test_clear_storage` -- reset localStorage/sessionStorage

### Running Tests

```bash
# Web E2E (Playwright)
yarn e2e

# Tauri E2E (Selenium)
cd e2e-tests && npm test
```

### Platform Support

- **Linux**: WebKitWebDriver via webkit2gtk-driver (primary CI target)
- **Windows**: EdgeDriver (supported)
- **macOS**: WKWebView lacks WebDriver -- use Playwright for web target

### Production Safety

Test commands compile only with `--features e2e`. No performance or binary size impact on release builds.

## Deep Link Testing (macOS)

```bash
# Test with direct open
open 'nearx://v1/tx/ABC123'
open 'nearx://v1/account/alice.near'

# View logs
tail -f ~/Library/Logs/com.fastnear.nearx/NEARx.log
```

The `tauri-dev.sh` script automates build, Launch Services registration, and test for macOS development.
