# NEARx Browser Extension + Native App Integration

Status: Active (manifest playbook)  
Last Updated: 2026-02-26

This document describes the intended 1Password-style architecture and provides a concrete manifest setup playbook.

## 1Password-Style Model (Authoritative Architecture)

There are three manifest layers:

1. Browser extension manifest
- Purpose: browser permissions, scripts, and extension identity.
- Canonical files: `extension/manifest.chrome.json`, `extension/manifest.firefox.json`.

2. Native messaging host manifest
- Purpose: tell browser where to launch the native host binary.
- Canonical template: `native-host/com.nearx.native.json`.

3. Tauri app manifest
- Purpose: desktop app settings, plugins, and deep-link scheme registration.
- File: `tauri-workspace/src-tauri/tauri.conf.json`.

These manifests are complementary and not interchangeable.

## Canonical Names and IDs

- Native host manifest name: `com.nearx.native`
- Native host template file: `native-host/com.nearx.native.json`
- Tauri deep-link scheme: `nearx`
- Firefox extension ID in repo: `nearx@native` (from `extension/manifest.firefox.json`)

## Manifest Setup Playbook (Step by Step)

### 1) Configure Tauri deep-link registration

Ensure `tauri-workspace/src-tauri/tauri.conf.json` contains:

```json
"plugins": {
  "deep-link": {
    "desktop": {
      "schemes": ["nearx"]
    }
  }
}
```

Runtime handling is in `tauri-workspace/src-tauri/src/main.rs`:

- deep-link plugin init
- `on_open_url` handler
- single-instance argv deep-link handling
- canonicalization via `canonicalize_deep_link` (broker-first path)

Important:

- packaged installs register the protocol with the OS
- `cargo tauri dev` supports runtime testing; Linux/debug paths use plugin registration helpers in code

Quick validation:

```bash
open 'nearx://v1/home'        # macOS
xdg-open 'nearx://v1/home'    # Linux
start nearx://v1/home         # Windows
```

### 2) Select extension manifest for your browser

Chrome/Edge build:

```bash
cp extension/manifest.chrome.json extension/manifest.json
```

Firefox build:

```bash
cp extension/manifest.firefox.json extension/manifest.json
```

Note:

- browsers load `extension/manifest.json`
- `manifest.chrome.json` and `manifest.firefox.json` are the canonical source manifests in this repo

### 3) Build the native host binary

```bash
cd native-host
cargo build --release
```

Typical binary paths:

- macOS/Linux: `native-host/target/release/nearx-native-host`
- Windows: `native-host\\target\\release\\nearx-native-host.exe`

### 4) Create browser-specific native host manifest files

Start from template:

```bash
cp native-host/com.nearx.native.json /ABS/PATH/com.nearx.native.json
```

For Chrome/Edge/Chromium keep:

- `allowed_origins`: `["chrome-extension://<EXTENSION_ID>/"]`

For Firefox change key to:

- `allowed_extensions`: `["nearx@native"]` (or your chosen Firefox extension ID)

Always set:

- `name`: `com.nearx.native`
- `path`: absolute path to native host executable
- `type`: `stdio`

### 5) Install native host manifest (OS + browser)

macOS:

- Chrome: `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.nearx.native.json`
- Chromium: `~/Library/Application Support/Chromium/NativeMessagingHosts/com.nearx.native.json`
- Edge: `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/com.nearx.native.json`
- Firefox: `~/Library/Application Support/Mozilla/NativeMessagingHosts/com.nearx.native.json`

Linux:

- Chrome (user): `~/.config/google-chrome/NativeMessagingHosts/com.nearx.native.json`
- Chromium (user): `~/.config/chromium/NativeMessagingHosts/com.nearx.native.json`
- Firefox (user): `~/.mozilla/native-messaging-hosts/com.nearx.native.json`

Windows:

- store manifest JSON anywhere on disk
- create registry default value pointing to that JSON:
  - Chrome: `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.nearx.native`
  - Edge: `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.nearx.native`
  - Firefox: `HKCU\Software\Mozilla\NativeMessagingHosts\com.nearx.native`

Example (PowerShell, Chrome):

```powershell
$manifest = "C:\Users\<you>\AppData\Local\nearx\com.nearx.native.json"
New-Item -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.nearx.native" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.nearx.native" -Name "(default)" -Value $manifest
```

### 6) Load the extension

Chrome/Edge:

1. Open `chrome://extensions` (or `edge://extensions`).
2. Enable Developer mode.
3. Load unpacked `extension/`.
4. Copy extension ID.
5. Confirm native host manifest `allowed_origins` includes `chrome-extension://<EXTENSION_ID>/`.

Firefox:

1. Open `about:debugging#/runtime/this-firefox`.
2. Click "Load Temporary Add-on".
3. Select `extension/manifest.json` (copied from `manifest.firefox.json`).
4. Confirm native host manifest uses `allowed_extensions` with your Firefox extension ID.

### 7) Run broker and desktop app

Terminal 1:

```bash
cd /Users/mikepurvis/near/fn/nearx
make nearxd
```

Terminal 2:

```bash
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri dev
```

### 8) Verify end-to-end

Deep-link path:

```bash
open 'nearx://v1/account/intents.near'      # macOS
xdg-open 'nearx://v1/account/intents.near'  # Linux
start nearx://v1/account/intents.near       # Windows
```

Extension-native path:

- trigger extension action that sends `open_deeplink`
- confirm background connects to `com.nearx.native`
- confirm native host returns `ok` or `data`

## Current Repository State

What exists:

- extension background forwards `open_deeplink` via `connectNative("com.nearx.native")`
- native host binary/protocol exists and is broker-first
- tauri app registers and handles `nearx://` deep links

What is not fully wired yet:

- richer structured authenticated workflows in extension UX
- biometric/user-presence gate for intent approval
- full daemon/native-host/desktop integration CI scenario

## Protocol Contract (Extension <-> Native Host)

Native messaging transport is length-prefixed JSON over stdio.

Implemented inbound message types in `native-host/src/main.rs`:

- `hello`
- `ping`
- `open_deep_link`
- `open_session`
- `create_sign_intent`
- `approve_sign_intent`
- `consume_sign_intent`

`create_sign_intent` forwarded params support:

- `account_id`, `payload`, optional `origin`, optional `expires_in_ms`
- optional `require_user_presence`, optional `user_presence_reason`

Outbound response types:

- `hello`
- `pong`
- `ok`
- `data`
- `err`

Legacy exception:

- `open_session` currently opens `near://open/session/<id>?readOnly=<0|1>`.
- Treat this as migration debt outside the strict `nearx://v1/...` contract.

Current host behavior is broker-first:

- tries local `nearxd` broker (`open_deep_link` method over unix socket)
- forwards sign-intent methods to `nearxd` and returns structured payloads (`type=data`)
- falls back to direct OS launcher (`open`, `xdg-open`, `rundll32`) only when broker is unavailable

## Recommended Integration Target

For 1Password-style UX:

1. content script detects actionable page context
2. content script sends intent to background
3. background uses `chrome.runtime.connectNative("com.nearx.native")`
4. native host forwards to running tauri app IPC (preferred), deep-link fallback for app focus/open
5. background returns structured result to content/page

## Security Notes

- Keep native host allowlist narrow (`allowed_origins` for Chromium browsers, `allowed_extensions` for Firefox).
- Prefer local authenticated IPC from native host to app for sensitive actions.
- Keep deep links for navigation/opening; use IPC protocol for privileged operations.

## Continuity Rules

When integration changes:

1. Update this file and `docs/DEEP_LINK_URI_SPEC.md` together.
2. Keep extension, native host, and tauri manifest names consistent.
3. Keep `near://` mentions limited to compatibility parsing and the explicit `open_session` migration path.
