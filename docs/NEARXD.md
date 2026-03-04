# nearxd Local Broker Daemon

Status: Active (Phase 3 user-presence and credential import)  
Last Updated: 2026-02-26

`nearxd` is the local broker/control-plane process for NEARx desktop integrations.

Goals:

- centralize privileged local flows behind one daemon boundary
- provide one local API for extension/native-host/app coordination
- enforce consistent deep-link and signing policy across all frontends
- support secure-storage and user-presence flows (macOS keychain + LocalAuthentication)

## Transport and Runtime

- Binary: `nearxd` (`src/bin/nearxd.rs`)
- Transport: newline-delimited JSON over Unix domain socket
- Default socket path (unix): `${TMPDIR:-/tmp}/nearxd.sock`
  - override with `NEARXD_SOCKET_PATH`

Request:

```json
{"id":"1","method":"ping","params":{}}
```

Response:

```json
{"id":"1","ok":true,"result":{"name":"nearxd","version":1}}
```

## Method Surface

Runtime/deep-link:

- `ping`
- `get_runtime_config`
- `parse_deep_link`
- `open_deep_link`

FastNEAR token:

- `resolve_fastnear_auth_token` (`get_fastnear_auth_token` alias)
- `resolve_fastnear_api_key` (`get_fastnear_api_key` alias)
- `set_fastnear_auth_token` (`set_fastnear_api_key` alias)
- `clear_fastnear_auth_token` (`clear_fastnear_api_key` alias)

User-presence and signing settings:

- `probe_user_presence`
- `request_user_presence`
- `get_signing_settings`
- `set_signing_settings`

Credential discovery and import:

- `list_near_credentials`
- `import_near_credentials`
- `get_near_credential`

Sign intent:

- `create_sign_intent`
- `approve_sign_intent`
- `consume_sign_intent`

Transaction signing:

- `sign_transaction`

## Storage Backends

FastNEAR token precedence:

1. daemon session token
2. persisted token store (`NEARXD_TOKEN_BACKEND`)
3. `FASTNEAR_API_KEY` env var (canonical)
4. `FASTNEAR_AUTH_TOKEN` env var (legacy alias)

Runtime request format:

- clients append key as query parameter: `?apiKey=<KEY>`

Token backend selection:

- `NEARXD_TOKEN_BACKEND=auto` (default)
  - macOS: keychain primary, file fallback
  - non-macOS: file
- `NEARXD_TOKEN_BACKEND=keychain` (macOS only)
- `NEARXD_TOKEN_BACKEND=file`

File paths:

- token file: `$NEARXD_TOKEN_FILE` or `~/.nearx/fastnear_auth_token` (filename retained for compatibility)
- signing settings fallback file: `~/.nearx/signing_settings.json`

Keychain services (macOS):

- `nearxd.fastnear.auth` (account: `fastnear_auth_token`)
- `nearxd.near.credentials` (account namespace: `<network>:<account_id>`)
- `nearxd.signing.settings` (account: `default`)

## User-Presence Adapter

Environment variable:

- `NEARXD_USER_PRESENCE_ADAPTER=auto|swift|mock|none`

Behavior:

- `auto`: use Swift `LocalAuthentication` adapter on macOS, unavailable elsewhere
- `swift`: force Swift adapter (macOS only)
- `mock`: always return verified=true (for tests/dev only)
- `none`: disable user-presence prompts

`request_user_presence` returns `ERR_AUTH` when prompt fails/rejected/unavailable.

## RPC Contract Details

### `probe_user_presence`

Input:

```json
{"allow_fallback": true}
```

Output:

```json
{
  "adapter": "swift",
  "platform": "macos",
  "available": true,
  "modality": "biometrics"
}
```

`modality` may be `biometrics`, `device_owner_authentication`, `mock`, or `none`.

### `request_user_presence`

Input:

```json
{
  "reason": "Approve NEAR signing request",
  "allow_fallback": true
}
```

Output:

```json
{
  "verified": true,
  "platform": "macos",
  "modality": "biometrics",
  "adapter": "swift"
}
```

### `set_signing_settings` / `get_signing_settings`

`set_signing_settings` input:

```json
{
  "settings": {
    "default_network": "testnet",
    "require_user_presence": true
  },
  "prefer_keychain": true
}
```

`set_signing_settings` output:

```json
{"stored": true, "source": "keychain"}
```

`get_signing_settings` output:

```json
{
  "settings": {
    "default_network": "testnet",
    "require_user_presence": true
  },
  "source": "keychain"
}
```

### `list_near_credentials`

Lists available credential accounts from `~/.near-credentials/<network>` (or custom dir). Returns account IDs and public keys only — never exposes private keys. Returns an empty accounts array (not an error) when the directory does not exist or contains no parseable credentials.

Input:

```json
{
  "network": "mainnet",
  "credentials_dir": "~/.near-credentials/mainnet"
}
```

- `network` — optional, defaults to `"mainnet"`
- `credentials_dir` — optional, overrides default `~/.near-credentials/<network>`

Output:

```json
{
  "network": "mainnet",
  "credentials_dir": "/Users/alice/.near-credentials/mainnet",
  "accounts": [
    { "account_id": "alice.near", "public_key": "ed25519:..." },
    { "account_id": "bob.near", "public_key": "ed25519:..." }
  ]
}
```

### `import_near_credentials`

Imports credentials from `~/.near-credentials/<network>` (or custom dir), optionally requiring user presence and persisting keys to keychain.

Input:

```json
{
  "network": "testnet",
  "credentials_dir": "~/.near-credentials/testnet",
  "account_id": "alice.testnet",
  "require_user_presence": true,
  "allow_fallback": false,
  "persist_in_keychain": true,
  "keychain_credential_protection": "biometry_current_set",
  "overwrite": false,
  "save_settings": true
}
```

Output (shape):

```json
{
  "network": "testnet",
  "credentials_dir": "/Users/you/.near-credentials/testnet",
  "imported_count": 1,
  "imported": [
    {
      "account_id": "alice.testnet",
      "public_key": "ed25519:...",
      "file": ".../alice.testnet.json",
      "keychain_account": "testnet:alice.testnet",
      "keychain_status": "stored"
    }
  ],
  "skipped": [],
  "failed": [],
  "user_presence": {
    "verified": true,
    "modality": "biometrics"
  },
  "settings_save": {
    "saved": true,
    "source": "keychain"
  }
}
```

Key hardening options:

- `keychain_credential_protection`:
  - `biometry_current_set` (default, strict Touch ID / biometric-bound access)
  - `user_presence` (allows device-owner auth policy)
- `allow_fallback` default is tied to protection mode:
  - `false` for `biometry_current_set`
  - `true` for `user_presence`

### `get_near_credential`

Reads an imported credential from keychain and requires user presence on each read.

Input:

```json
{
  "network": "testnet",
  "account_id": "alice.testnet",
  "reason": "NEARx needs your approval to access this credential."
}
```

Output (shape):

```json
{
  "network": "testnet",
  "account_id": "alice.testnet",
  "credential": {
    "network": "testnet",
    "account_id": "alice.testnet",
    "public_key": "ed25519:...",
    "private_key": "ed25519:...",
    "keychain_account": "testnet:alice.testnet"
  }
}
```

### Sign intent + user presence

`create_sign_intent` supports:

- `require_user_presence` (bool, default `false`)
- `user_presence_reason` (string, optional)

When enabled, `approve_sign_intent` performs user-presence verification before setting `status=approved`.

### `sign_transaction`

Signs a NEAR transaction using a credential stored in the macOS keychain (triggers Touch ID).

Request params:

| Param | Type | Required | Description |
|---|---|---|---|
| `signer_id` | string | yes | NEAR account ID of the signer |
| `receiver_id` | string | yes | NEAR account ID of the receiver |
| `nonce` | u64 | yes | Access key nonce (typically `current_nonce + 1`) |
| `block_hash` | string | yes | Recent block hash (base58) |
| `actions` | array | yes | Array of action objects (see below) |
| `network` | string | no | `"mainnet"` or `"testnet"` (default: `"mainnet"`) |
| `reason` | string | no | Touch ID prompt reason |

Action types:

- `{"type": "Transfer", "deposit": "<yoctoNEAR>"}` — native NEAR transfer
- `{"type": "FunctionCall", "method_name": "...", "args": "<base64>", "gas": <u64>, "deposit": "<yoctoNEAR>"}` — contract call (gas defaults to 30 TGas, deposit defaults to "0")

Response:

```json
{
  "signed_transaction_base64": "...",
  "tx_hash": "<base58>",
  "signer_id": "alice.near",
  "public_key": "ed25519:..."
}
```

The `signed_transaction_base64` is borsh-serialized, ready for `broadcast_tx_commit`.

## Native Host and Tauri Integration

Native host:

1. call `nearxd.open_deep_link`
2. if broker unavailable, fallback to direct OS open (`open`/`xdg-open`/`rundll32`)
3. validation failures from broker do not fallback

Tauri:

1. incoming URL / argv is canonicalized through `nearxd.parse_deep_link` (when available)
2. canonical URI is emitted to webview on `nearx://open`
3. web worker applies route with shared router
4. runtime config is sourced from `nearxd.get_runtime_config(include_token=true)` when available

## Manual QA (macOS)

Use a stable local socket path:

```bash
export NEARXD_SOCKET_PATH=/tmp/nearxd.sock
export NEARXD_USER_PRESENCE_ADAPTER=swift
make nearxd
```

In another terminal:

```bash
printf '{"id":"1","method":"probe_user_presence","params":{}}\n' | nc -U /tmp/nearxd.sock
printf '{"id":"2","method":"request_user_presence","params":{"reason":"NEARx QA: user presence test","allow_fallback":false}}\n' | nc -U /tmp/nearxd.sock
```

Or run the helper script:

```bash
./scripts/nearxd-qa-macos.sh testnet ~/.near-credentials/testnet alice.testnet
```

Credential import test:

```bash
printf '{"id":"3","method":"import_near_credentials","params":{"network":"testnet","credentials_dir":"~/.near-credentials/testnet","require_user_presence":true,"persist_in_keychain":true,"save_settings":true}}\n' | nc -U /tmp/nearxd.sock
```

If no prompt appears:

- ensure `nearxd` runs in the logged-in GUI user session
- ensure Xcode command line tools are installed (`swift` present)
- check adapter mode (`NEARXD_USER_PRESENCE_ADAPTER`)

## Why This Architecture

- avoids duplicating security-sensitive logic across extension/web/tauri glue
- gives one stable local API for all clients
- allows incremental rollout to keychain + user-presence without breaking navigation flows
