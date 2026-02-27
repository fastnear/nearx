# Chapter 7: Testing & Security

This chapter covers end-to-end testing procedures, OAuth authentication, and security measures implemented in NEARx.

## End-to-End Testing (E2E)

### Technology Stack
Selenium WebDriver + tauri-driver for desktop automation

### Overview
Production-ready E2E test suite that validates critical integration points without brittle canvas pixel inspection. Uses Tauri's official WebDriver stack for Linux/Windows desktop testing.

### Platform Support
- ✅ **Linux**: WebKitWebDriver via webkit2gtk-driver (primary CI target)
- ✅ **Windows**: EdgeDriver (supported, CI optional)
- ❌ **macOS**: Not supported (WKWebView lacks WebDriver) - use Playwright for web target instead

### Test Architecture

The E2E system uses a **three-layer testing API**:

1. **Test-only IPC commands** (`e2e` feature flag)
   - `nearx_test_emit_deeplink` - Inject deep link events without OS registration
   - `nearx_test_get_last_route` - Query routing state
   - `nearx_test_clear_storage` - Reset localStorage/sessionStorage

2. **JavaScript test bridge** (`window.NEARxTest`)
   - Route tracking, clipboard simulation, keyboard events
   - Deep link event history
   - OAuth token management

3. **Selenium WebDriver** (standard DOM/script execution)
   - Canvas size verification
   - Event dispatch and assertions
   - Async command invocation

### Test Suites

The `e2e-tests/test/smoke.spec.mjs` suite covers:

1. **Rendering & Layout**
   - Canvas renders and fills viewport (>400x300px)
   - NEARxTest bridge availability

2. **OAuth Router**
   - Simulates user landing on #/auth/callback?token=e2e-token
   - Validates URL scrubbing and token persistence

3. **Deep Link Bridge**
   - Uses test IPC to inject nearx://v1/tx/HASH
   - Validates event emission and route updates

4. **Clipboard Integration**
   - Simulates 'c' key to copy focused pane
   - Validates Tauri clipboard plugin functionality

5. **Keyboard & Mouse Navigation**
   - Tab key cycling through panes
   - Cursor state tracking

6. **Storage & State**
   - Clear storage via test API
   - Verify localStorage persistence

7. **Error Handling**
   - Malformed deep links don't crash app
   - Graceful degradation

### Test-Only API Reference

**Rust IPC Commands** (only when built with `--features e2e`):

```rust
// tauri-workspace/src-tauri/src/test_api.rs

#[tauri::command]
async fn nearx_test_emit_deeplink(app: AppHandle, url: String) -> Result<(), String>

#[tauri::command]
async fn nearx_test_get_last_route() -> Result<String, String>

#[tauri::command]
async fn nearx_test_clear_storage(window: Window) -> Result<(), String>
```

**JavaScript Bridge** (`window.NEARxTest`):

```javascript
// Route tracking
getLastRoute()              // Returns last navigated route
waitForRoute(route, ms)     // Async wait for specific route

// Deep links
getDeepLinkHistory()        // Array of {timestamp, url}
clearDeepLinkHistory()      // Reset history

// Clipboard
copyFocused()               // Simulate 'c' key press

// OAuth
getToken()                  // Get from localStorage
setToken(token)             // Set in localStorage

// Keyboard
pressKey('Tab')             // Dispatch KeyboardEvent

// Cursor
cursorIsPointer()           // Check hover state

// Tauri commands
invoke(cmd, args)           // Wrapper for __TAURI__.invoke
```

### Running Tests Locally

**Standard workflow:**
```bash
cd e2e-tests
npm test  # Builds app + runs tests
```

**Manual control** (two terminals):
```bash
# Terminal 1: Build app with e2e features
cd tauri-workspace
cargo tauri build --debug --no-bundle --features e2e

# Terminal 2: Start tauri-driver
tauri-driver

# Terminal 3: Run tests
cd e2e-tests
npm test
```

**Linux headless** (CI simulation):
```bash
xvfb-run -a npm test
```

### CI Integration

See `.github/workflows/e2e.yml` for GitHub Actions configuration.

**Key steps:**
1. Install webkit2gtk-driver + xvfb (Linux)
2. Build app: `cargo tauri build --debug --no-bundle --features e2e`
3. Install tauri-driver: `cargo install tauri-driver --locked`
4. Run tests: `xvfb-run -a npm test`

### Production Safety

**Zero risk to production:**
- Test commands only compiled with `--features e2e`
- NEARxTest bridge only loaded in test builds
- Feature flag checked at compile time via `#[cfg(feature = "e2e")]`
- No performance or binary size impact on release builds

## OAuth & Authentication

NEARx v0.4.2 introduces production-ready OAuth integration for Web and Tauri targets, enabling secure user authentication with Google OAuth and Magic link providers.

### Architecture

**Token Storage**: All targets use webview localStorage with key `nearx.token`
- **Web**: Browser localStorage (persists across sessions)
- **Tauri**: Webview storage shared with native backend
- **Priority**: User token → Environment token → None

**Authentication Flow:**
1. User clicks "Sign in with Google" or "Magic link"
2. **Web**: OAuth popup window opens
3. **Tauri**: System browser opens via Opener plugin (isolates OAuth from app)
4. Provider redirects to callback: `#/auth/callback?token=<jwt>` (Web) or `nearx://auth/callback?token=<jwt>` (Tauri)
5. Router shim extracts token, persists to localStorage
6. URL scrubbed to `#/` (prevents token leaks via browser history/sharing)

### Implementation (`src/auth.rs`)

```rust
pub struct AuthState {
    pub token: Option<String>,
    pub email: Option<String>,      // Optional, if backend returns it
    pub provider: Option<String>,   // "google" | "magic"
}

// Core API
pub fn set_token(token: String, provider: Option<String>, email: Option<String>);
pub fn clear();
pub fn has_token() -> bool;        // Returns true for non-empty tokens
pub fn token_string() -> Option<String>;
pub fn attach_auth(rb: RequestBuilder) -> RequestBuilder;  // Adds Bearer token to requests

// Callback handler (supports token= or code= query params)
pub fn handle_auth_callback_query(qs: &str);
```

### Router Shim (`web/router_shim.js`)

Listens for hash changes and processes auth callbacks:

```javascript
// Triggered on: window.location.hash = "#/auth/callback?token=..."
if (hash.startsWith('#/auth/callback')) {
    const qs = hash.split('?')[1] || '';
    window.NEARxAuth?.handleCallback(qs);  // Calls Rust auth::handle_auth_callback_query()

    // Scrub URL to prevent leaks
    history.replaceState(null, '', '#/');
}
```

**Tauri Deep Link**: `nearx://auth/callback?token=...` handled by deep link system, routed to same callback logic.

### OAuth Providers

**Google OAuth (PKCE Flow):**
- Client ID configured in auth backend
- Scopes: `openid email profile`
- Redirect URI: `https://your-app.com/#/auth/callback` (Web) or `nearx://auth/callback` (Tauri)

**Magic Links:**
- Backend sends passwordless email link
- User clicks link → callback with `token=<jwt>`

### Security Features

**Token Handling:**
- ✅ Never logged or exposed in console (verified via git grep)
- ✅ URL scrubbed immediately after extraction (prevents history leaks)
- ✅ Stored in localStorage only (secure, HttpOnly not needed for client-side apps)
- ✅ CSP headers block XSS attacks (see Security section below)

**Tauri Isolation:**
- System browser used for OAuth (Opener plugin)
- Prevents credential phishing via fake webview
- Deep link callback returns control to app after authentication

### Testing OAuth

**Web (Deterministic Test):**
```bash
make dev
# In browser: paste URL
http://localhost:8000/#/auth/callback?token=smoke-token

# Expected:
# - Hash scrubs to #/
# - localStorage.getItem('nearx.token') === 'smoke-token'
# - Console: [NEARx][auth] token set
```

**Tauri (Live OAuth):**
```bash
cargo tauri dev
# Click "Sign in with Google"
# Complete OAuth flow in system browser
# Deep link returns to app, token persisted
```

**E2E Tests**: See `e2e-tests/test/smoke.spec.mjs` for automated OAuth flow validation.

## Security

NEARx v0.4.2 implements defense-in-depth security with CSP headers, XSS hardening, and secure token handling.

### Content Security Policy (CSP)

**Web (`index.html`):**
```html
<meta http-equiv="Content-Security-Policy" content="
    default-src 'none';
    script-src 'self';
    style-src 'self' 'unsafe-inline';
    img-src 'self' data:;
    connect-src 'self' https://accounts.google.com https://oauth2.googleapis.com
                https://rpc.mainnet.near.org https://rpc.testnet.near.org
                https://rpc.mainnet.fastnear.com https://rpc.testnet.fastnear.com
                https://archival-rpc.mainnet.fastnear.com
                https://archival-rpc.testnet.fastnear.com
                https://*.near.org http://localhost:* ws: wss:;
    font-src 'self';
    base-uri 'none';
    frame-ancestors 'none';
">
```

**Tauri (`tauri.conf.json`):**
Same CSP policy mirrored in Tauri configuration.

**CSP Directives:**
- `default-src 'none'` - Deny by default, explicit allow only
- `script-src 'self'` - Only scripts from same origin (blocks inline scripts, eval, remote scripts)
- `style-src 'self' 'unsafe-inline'` - Allows inline styles for theme system
- `connect-src` - Whitelisted OAuth providers, NEAR RPCs, WebSocket (ws:/wss:)
- `frame-ancestors 'none'` - Prevents clickjacking
- `base-uri 'none'` - Prevents base tag injection

**Development vs Production:**
- Development: `http://localhost:*` allowed for local testing
- Production: Remove localhost, keep `https:` + `wss:` only

### XSS Mitigation

**Token Protection:**
- CSP blocks inline scripts (prevents `<script>` injection)
- CSP blocks `eval()` and `Function()` (prevents code injection)
- URL scrubbing prevents token leaks via browser history

**Input Sanitization:**
- Filter queries validated before execution
- JSON rendering uses syntax highlighting (no raw HTML injection)
- All user input escaped in UI rendering

### Verification

```bash
# Check for token leaks in logs
git grep -nE 'Authorization:|x-api-key|token=' -- ':!tests/*' || echo "✅ Clean"

# Verify CSP in production build
make web-release
grep 'Content-Security-Policy' web/index.html
```

## Comparison: E2E vs Unit vs Integration Tests

| Test Type | Scope | Speed | When to Use |
|-----------|-------|-------|-------------|
| **Unit** | Single function/module | ⚡ Instant | Logic validation, edge cases |
| **Integration** | Multiple modules | 🟡 Fast | API contracts, module interactions |
| **E2E (this)** | Full app + OS | 🔴 Slow | Deep links, clipboard, OAuth, rendering |

**When to add E2E tests:**
- ✅ System integration paths (deep links, clipboard, OAuth callbacks)
- ✅ Platform-specific behaviors (Tauri plugins, OS APIs)
- ✅ Critical user flows that cross multiple subsystems
- ❌ Business logic (use unit tests)
- ❌ Rendering pixel perfection (use visual regression if needed)

## Next Steps

- For reference and troubleshooting, see [Chapter 8: Reference](08-reference.md)
- For getting started, see [Chapter 1: Getting Started](01-getting-started.md)