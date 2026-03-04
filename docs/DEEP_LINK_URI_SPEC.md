# NEARx Deep Link URI Specification (Draft)

Status: Draft  
Version: `v1`  
Last Updated: 2026-02-26

## 1. Purpose

Define one strict, versioned URI contract that can be:

- opened by OS deep links in Tauri (`nearx://...`)
- passed to native CLI/TUI
- represented in web hash routes
- used by extension/native-host plumbing

This document is the parser and validation source of truth before implementation changes.

## 2. Canonical Form

Canonical URIs MUST use:

- scheme: `nearx`
- version segment: `v1`
- lowercase route names

Canonical template:

`nearx://v1/<route>`

Canonical routes:

- `nearx://v1/home`
- `nearx://v1/tx/<txHash>`
- `nearx://v1/block/<blockRef>`
- `nearx://v1/account/<accountId>`
- `nearx://v1/contract/<accountId>`
- `nearx://v1/access-key/<accountId>/<publicKey>`

## 3. Route Definitions

### 3.1 `home`

`nearx://v1/home`

Behavior:

- clear filters
- return to auto-follow
- keep pane defaults

### 3.2 `tx`

`nearx://v1/tx/<txHash>[?block=<blockRef>][&network=<network>]`

Behavior:

- focus transactions pane
- primary target is `txHash`
- if `block` is provided, use it as a selection hint (faster resolution)

### 3.3 `block`

`nearx://v1/block/<blockRef>[?network=<network>]`

Behavior:

- focus blocks pane
- lock selection to requested block
- disable auto-follow while locked

`<blockRef>` accepts either:

- block height (decimal u64)
- block hash (base58)

### 3.4 `account`

`nearx://v1/account/<accountId>[?network=<network>]`

Behavior:

- focus transactions pane
- apply account-scoped filtering (`acct:<accountId>`)

### 3.5 `contract`

`nearx://v1/contract/<accountId>[?method=<methodName>][&network=<network>]`

Behavior:

- focus transactions pane
- contract-centric filter intent:
  - receiver is contract account
  - contract-related actions (`FunctionCall`, `DeployContract`)
- optional `method` narrows function-call method

### 3.6 `access-key`

`nearx://v1/access-key/<accountId>/<publicKey>[?network=<network>]`

Behavior:

- focus transactions pane
- key-centric filter intent:
  - account scope
  - key material scope
  - key-management actions (`AddKey`, `DeleteKey`)

`<publicKey>` is the NEAR key string, e.g. `ed25519:...` or `secp256k1:...`.

## 4. Query Parameters

Allowed query keys are route-specific and strict.

Global:

- `network`: `mainnet` | `testnet` | `betanet` | `localnet` | `custom`

Route-specific:

- `tx`: `block`
- `contract`: `method`

Unknown query keys MUST cause validation failure.

## 5. Validation Rules

### 5.1 Common

- URI MUST parse as absolute URI.
- Scheme MUST be `nearx` after normalization.
- Route version MUST be exactly `v1`.
- Percent-decoding MUST happen once.

### 5.2 `txHash` and `blockHash`

- MUST be base58
- MUST be 43-44 chars
- regex: `^[1-9A-HJ-NP-Za-km-z]{43,44}$`

### 5.3 `blockHeight`

- decimal, no sign, no separators
- range: `1..=u64::MAX`

### 5.4 `accountId`

Accepted forms:

- named account ID (NEAR account format, lowercase)
- implicit account ID (64 hex chars)

Minimum enforcement:

- length `2..=64`
- lowercase only for named account IDs

### 5.5 `publicKey`

- format: `<curve>:<base58>`
- curves: `ed25519` | `secp256k1`
- regex: `^(ed25519|secp256k1):[1-9A-HJ-NP-Za-km-z]{32,128}$`

### 5.6 `methodName`

- non-empty
- max 128 chars
- regex: `^[A-Za-z0-9_.$-]{1,128}$`

## 6. Canonicalization

Inputs MAY arrive in non-canonical form. Canonicalization MUST:

1. trim whitespace
2. map legacy scheme `near:` to `nearx:`
3. lowercase scheme, version, and route token
4. preserve case-sensitive identifiers (`txHash`, `blockHash`, key payload)
5. remove empty trailing slash from route
6. sort query keys lexicographically
7. emit canonical URI string

## 7. Compatibility Aliases (Input Only)

These MAY be accepted but MUST normalize to canonical `nearx://v1/...`.

- `nearx://tx/<txHash>` -> `nearx://v1/tx/<txHash>`
- `nearx://tx/<blockRef>/<txHash>` -> `nearx://v1/tx/<txHash>?block=<blockRef>`
- `nearx://block/<blockRef>` -> `nearx://v1/block/<blockRef>`
- `nearx://account/<accountId>` -> `nearx://v1/account/<accountId>`
- `nearx://contract/<accountId>` -> `nearx://v1/contract/<accountId>`
- `nearx://access-key/<accountId>/<publicKey>` -> `nearx://v1/access-key/<accountId>/<publicKey>`

Legacy:

- `near://...` accepted only as compatibility input, never emitted.

## 8. Transport Across Targets

### 8.1 Tauri / OS Deep Link

Use canonical full URI:

`nearx://v1/...`

### 8.2 Web Hash Route

Two supported representations:

- direct path hash: `#/v1/...`
- encoded full URI hash: `#/deeplink/<encodeURIComponent(nearx://v1/...)>`

### 8.3 Native TUI/CLI

Pass full canonical URI as CLI arg:

`nearx nearx://v1/...`

## 9. Parsing Precedence

For `<blockRef>`:

1. all-digits -> treat as height
2. base58 hash regex match -> treat as hash
3. otherwise -> validation error

For alias `tx/<a>/<b>`:

- `a` MUST parse as `blockRef`
- `b` MUST parse as `txHash`

## 10. Error Model

Parser SHOULD expose stable error categories:

- `ERR_SCHEME`
- `ERR_VERSION`
- `ERR_ROUTE`
- `ERR_SEGMENT_COUNT`
- `ERR_QUERY_KEY`
- `ERR_IDENTIFIER_FORMAT`
- `ERR_DECODE`

On validation failure:

- no partial route application
- no silent fallback to unrelated route

## 11. Conformance Examples

Valid canonical:

- `nearx://v1/home`
- `nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b`
- `nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b?block=178923456`
- `nearx://v1/block/178923456`
- `nearx://v1/block/9xQeWvG816bUx9EPf6hQYy7x6jQW2hRymP7vQ3P7Bh6P`
- `nearx://v1/account/intents.near`
- `nearx://v1/contract/app.near?method=ft_transfer`
- `nearx://v1/access-key/app.near/ed25519:4NfTivU6N3Wy6u9k8i8w8s8qF2g8Xx1R5zqjYQz6YJ8a`

Valid non-canonical input (must normalize):

- `nearx://tx/178923456/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b`
- `near://block/178923456`

Invalid:

- `nearx://v2/tx/...` (unsupported version)
- `nearx://v1/access-key/app.near/not-a-key`
- `nearx://v1/contract/App.NEAR` (uppercase named account)
- `nearx://v1/tx/<hash>?foo=bar` (unknown query key)
