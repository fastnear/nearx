# NEARx QA Playbook

Status date: 2026-02-27

This is the single best QA checklist for the highest-impact repository changes:

- strict deep-link routing + canonicalization
- `nearxd` broker contract and secure-storage hardening
- Tauri runtime wiring and terminal-look parity
- Google OAuth + magic-link auth entrypoints
- extension/native-host broker forwarding
- archival endpoint coverage with FastNear RPCs

## 1) FastNear Key Setup (Canonical)

Preferred env var:

- `FASTNEAR_API_KEY=<your_key>`

Legacy alias (still supported):

- `FASTNEAR_AUTH_TOKEN=<your_key>`

Runtime resolution order:

1. nearxd in-memory session key
2. nearxd persisted key (keychain/file)
3. `FASTNEAR_API_KEY`
4. `FASTNEAR_AUTH_TOKEN` (legacy)

Optional broker set via RPC:

```bash
printf '{"id":"1","method":"set_fastnear_api_key","params":{"token":"YOUR_KEY","persist":true}}\n' | nc -U /tmp/nearxd.sock
```

## 2) Launch Core Targets

```bash
# terminal 1
cd /Users/mikepurvis/near/fn/nearx
make nearxd

# terminal 2
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri dev
```

## 3) Deep-Link Validation (Tauri + Broker)

### macOS precondition checkpoint

Before testing deep links on macOS, ensure Launch Services points to a fresh bundle:

```bash
cd /Users/mikepurvis/near/fn/nearx/tauri-workspace
cargo tauri build --debug --bundles app --no-sign
ditto target/debug/bundle/macos/NEARx.app /Applications/NEARx.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/NEARx.app
mdfind "kMDItemCFBundleIdentifier == 'com.fastnear.nearx'"
```

### Deep-link test commands

```bash
open 'nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b'
open 'nearx://v1/block/178923456'
open 'nearx://v1/contract/intents.near?method=execute'
open 'nearx://v1/access-key/intents.near/ed25519:11111111111111111111111111111111'
```

Pass criteria:

- URL canonicalizes to `nearx://v1/...`
- app opens single instance and routes correctly
- no invalid-route fallback to unsafe behavior
- deep links open the current build (not a stale legacy UI build)

## 4) Auth UX Validation (Google + Magic Link)

In Tauri top bar:

- `Google` starts OAuth flow in system browser
- `Magic` prompts for email and triggers passwordless flow
- `Sign out` clears local token and runtime auth state

Pass criteria:

- callback returns to app route and token is persisted in webview storage
- auth status text updates (guest/signed-in)
- note: email-to-account auto-mapping is a planned follow-up, not part of current pass criteria

## 5) nearxd Strict User-Presence Validation (macOS)

```bash
export NEARXD_SOCKET_PATH=/tmp/nearxd.sock
export NEARXD_USER_PRESENCE_ADAPTER=swift
./scripts/nearxd-qa-macos.sh testnet ~/.near-credentials/testnet alice.testnet
```

Pass criteria:

- biometric/device-owner prompt appears
- credential import succeeds with keychain protection
- credential read requires prompt each time

## 6) Extension + Native Host Bridge

Follow `EXTENSION_SETUP.md` exactly, then confirm:

- extension sends `open_deep_link` via native messaging host
- native host forwards to `nearxd`
- sign-intent broker calls return structured responses

## 7) Archival Endpoint Validation

FastNear docs-aligned URLs:

- `https://archival-rpc.mainnet.fastnear.com`
- `https://archival-rpc.testnet.fastnear.com`

Pass criteria:

- old tx and historical block lookups succeed using archival fallback path
- requests include `apiKey` query parameter when key is configured

Reference docs:

- `docs/DEEP_LINK_URI_SPEC.md`
- `docs/DEEP_LINKS.md`
- `docs/NEARXD.md`
- `EXTENSION_SETUP.md`
