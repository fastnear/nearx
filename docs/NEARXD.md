# nearxd Local Broker Daemon

Status: Active (Phase 5 staking watchlist + Ledger beta)  
Last Updated: 2026-03-08

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

- `list_near_signing_accounts`
- `list_near_signing_keys`
- `import_near_signing_keys`
- `reprotect_near_signing_key`
- `list_near_credentials`
- `import_near_credentials`
- `get_near_credential`

Staking watchlist + hardware wallets:

- `list_staking_watchlist`
- `add_staking_watchlist_account`
- `remove_staking_watchlist_account`
- `connect_hardware_wallet`

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
- `nearxd.near.credentials` (account namespace: `<network>:<account_id>:<public_key>`, with backward read support for `<network>:<account_id>`)
- `nearxd.signing.settings` (account: `default`)

For NEAR signer credentials on macOS, NEARx now tracks whether a nearxd Keychain item is verified biometric (`biometry_current_set`) versus legacy/unknown. Importing into Keychain is additive only and does not remove the original file-system or near-cli secure source.

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

Additional signing-settings sections used by staking/hardware flows:

- `staking_watchlist`
  - `version`
  - `entries[]`: `{ network, account_id, added_at_ms, source }`
- `hardware_wallet_index`
  - `version`
  - `records[]`: `{ network, account_id, public_key, wallet_type, derivation_path, last_seen_at_ms }`

### `list_near_signing_accounts`

Lists signer accounts discoverable from:

- legacy credentials under `credentials_home_dir/<network>`
- near-cli secure account history (`accounts.json`)
- nearxd signing settings key index

Input:

```json
{
  "network": "mainnet",
  "credentials_home_dir": "~/.near-credentials"
}
```

`credentials_home_dir` defaults to near-cli config (`~/Library/Application Support/near-cli/config.toml` on macOS) and falls back to `~/.near-credentials`.

### `list_near_signing_keys`

Lists signer keys per account with permission summary, curve, and source availability.

Output rows include:

- `account_id`
- `public_key`
- `curve_type` (`ed25519`, `secp256k1`, `unknown`)
- `permission` (`full_access`, `function_call`, `unknown`)
- `available_sources` (`nearxd_keychain`, `near_cli_secure`, `legacy_file`, `hardware_wallet`)
- `preferred_source`
- `in_nearxd_keychain`
- `nearxd_keychain_protection` (`biometry_current_set`, `user_presence`, `unprotected`, `unknown`, or `null`)
- `nearxd_keychain_import_required`
- `importable`
- `last_seen_at_ms`
- `stale`

When RPC key discovery is unavailable, nearxd falls back to secure-keychain account enumeration for `near_cli_secure` keys on macOS.

On macOS, `nearxd_keychain_import_required=true` means the key has a nearxd Keychain copy but NEARx does not yet consider that copy verified for biometric signing.

In local ad-hoc or otherwise unsigned macOS builds, fingerprint-protected Keychain writes may fall back to unprotected Keychain storage. When that happens, NEARx should keep weaker local sources such as `legacy_file` or `near_cli_secure` available and prefer them over reopening the signer on blocked `nearxd_keychain`.

### `import_near_signing_keys`

Imports keys at key-level scope from one or more sources (`legacy_file`, `near_cli_secure`) into nearxd keychain and updates signing key index metadata.

Import is additive: original `legacy_file` and `near_cli_secure` sources remain intact and continue to appear in `available_sources`.

Supports optional filters:

- `account_id`
- `public_key`
- `source` / `sources`

Optional protection-related request fields:

- `keychain_credential_protection`
- `allow_fallback`
- `overwrite`

Per-row response fields include:

- `keychain_status`
- `keychain_protection`
- `storage_backend`

On macOS, if a protected write falls back to an unprotected Keychain write, the response reports `keychain_protection=unprotected` and later `list_near_signing_keys` will surface `nearxd_keychain_import_required=true`.

Fingerprint-signing QA for macOS must be run against a real signed app bundle and signed `nearxd` sidecar, not ad-hoc `target/debug` binaries.

### `reprotect_near_signing_key`

Repairs an existing nearxd Keychain credential in place so it is re-written with biometric Keychain protection on macOS.

Input:

```json
{
  "network": "mainnet",
  "account_id": "alice.near",
  "public_key": "ed25519:...",
  "reason": "Optional prompt reason"
}
```

Response fields include `keychain_status`, `keychain_protection`, and `storage_backend`.

### `list_near_credentials` (compatibility wrapper)

Legacy compatibility wrapper for callers expecting legacy-file credential listing. Internally uses the new discovery path but preserves old response shape.

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

### `import_near_credentials` (compatibility wrapper)

Legacy compatibility wrapper that maps to `import_near_signing_keys` with source restricted to `legacy_file`.

### `list_staking_watchlist`

Lists monitored staking accounts for a network from signing settings.

Input:

```json
{
  "network": "testnet"
}
```

Output:

```json
{
  "network": "testnet",
  "entries": [
    {
      "network": "testnet",
      "account_id": "alice.testnet",
      "added_at_ms": 1762400000000,
      "source": "manual"
    }
  ]
}
```

### `add_staking_watchlist_account` / `remove_staking_watchlist_account`

`add_staking_watchlist_account` validates account ID format (including implicit IDs), upserts an entry, and persists settings.

`remove_staking_watchlist_account` removes one account entry for the given network.

Input:

```json
{
  "network": "testnet",
  "account_id": "alice.testnet",
  "source": "manual",
  "prefer_keychain": false
}
```

`source` is `manual` or `seeded` (default: `manual`).

### `connect_hardware_wallet`

Connects a hardware wallet key for a specific account (Ledger Beta, macOS-first):

1. fetch public key from device for derivation path
2. validate that key exists on-chain for selected account
3. persist record to `hardware_wallet_index`
4. update signing key index with `hardware_wallet` source

Input:

```json
{
  "network": "testnet",
  "wallet_type": "ledger",
  "account_id": "alice.testnet",
  "derivation_path": "44'/397'/0'/0'/1'",
  "display_confirm": true
}
```

Output includes the connected key as a signing-key row shape plus `derivation_path`.

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
      "keychain_account": "testnet:alice.testnet:ed25519:...",
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

If `public_key` is provided and nearxd falls back to legacy account-scoped keychain storage, nearxd enforces strict key matching and rejects mismatches.

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
    "keychain_account": "testnet:alice.testnet:ed25519:..."
  }
}
```

### Sign intent + user presence

`create_sign_intent` supports:

- `require_user_presence` (bool, default `false`)
- `user_presence_reason` (string, optional)

When enabled, `approve_sign_intent` performs user-presence verification before setting `status=approved`.

### `sign_transaction`

Signs a NEAR transaction using a credential source selected from:

- `nearxd_keychain`
- `near_cli_secure`
- `legacy_file`
- `hardware_wallet`

`sign_transaction` accepts optional key/source selectors:

- `signer_public_key`
- `credential_source`

Source resolution defaults to:

1. `nearxd_keychain`
2. `near_cli_secure`
3. fail with import recommendation

When `credential_source=hardware_wallet`, `signer_public_key` is required and nearxd signs with the indexed hardware wallet record only (no automatic source fallback).

On macOS, when `credential_source=nearxd_keychain` is sent explicitly, nearxd rejects the request with `ERR_IMPORT_REQUIRED` unless that key's indexed `nearxd_keychain_protection` is `biometry_current_set`.

Response includes `credential_source` with the source actually used.

Request params:

| Param | Type | Required | Description |
|---|---|---|---|
| `signer_id` | string | yes | NEAR account ID of the signer |
| `signer_public_key` | string | no* | Explicit signer key (`required` for `credential_source=hardware_wallet`) |
| `credential_source` | string | no | `nearxd_keychain`, `near_cli_secure`, `legacy_file`, or `hardware_wallet` |
| `receiver_id` | string | yes | NEAR account ID of the receiver |
| `nonce` | u64 | yes | Access key nonce (typically `current_nonce + 1`) |
| `block_hash` | string | yes | Recent block hash (base58) |
| `actions` | array | yes | Array of action objects (see below) |
| `network` | string | no | `"mainnet"` or `"testnet"` (default: `"mainnet"`) |
| `reason` | string | no | Optional signing context/prompt reason (used by interactive credential sources when applicable) |

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

Hardware-specific failures use explicit error codes from broker responses:

- `ERR_HARDWARE_UNAVAILABLE`
- `ERR_HARDWARE_APP_NOT_OPEN`
- `ERR_HARDWARE_USER_REJECTED`
- `ERR_HARDWARE_KEY_NOT_ON_ACCOUNT`
- `ERR_HARDWARE_INVALID_PATH`
- `ERR_HARDWARE_TRANSPORT`
- `ERR_UNAVAILABLE`

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
