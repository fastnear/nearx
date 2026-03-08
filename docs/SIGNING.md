# macOS Code Signing & Notarization

This document covers the signing infrastructure for the NEARx Tauri desktop app. Signing is required for:

- Suppressing macOS Gatekeeper warnings ("unidentified developer")
- Enabling keychain access with biometric protection (LocalAuthentication)
- Distribution outside the Mac App Store via Developer ID

## Apple Developer Account

- **Account type**: Individual
- **Team ID**: `J49WU3CJQA`

## Prerequisites

1. **Apple Developer Program** enrollment ($99/year) at [developer.apple.com](https://developer.apple.com/programs/)
2. One of:
   - **Apple Development** certificate for local signed QA builds
   - **Developer ID Application** certificate for release/distribution builds
3. **App-specific password** or **App Store Connect API key** for notarization (release/distribution only)

## Build Types

### Local signed QA build

- Purpose: test biometric Keychain / fingerprint behavior locally
- Acceptable signing identities:
  - `Apple Development: ...`
  - `Developer ID Application: ...`
- Notarization: not required
- Gatekeeper (`spctl`): may still reject `Apple Development` builds, which is acceptable for local QA

Recommended command:

```bash
yarn build:macos-qa
open tauri-workspace/target/debug/bundle/macos/NEARx.app
```

The script:

- prefers `APPLE_SIGNING_IDENTITY` if set
- otherwise auto-picks `Developer ID Application` first, then `Apple Development`
- rebuilds `web/dist`
- rebuilds the bundled `nearxd` sidecar
- runs `cargo tauri build --debug --bundles app`
- fails if either the app bundle or bundled `nearxd` is still ad-hoc signed

### Release / distribution build

- Purpose: shareable release artifact outside local development
- Required signing identity: `Developer ID Application: ...`
- Required follow-up: notarization + stapling
- Gatekeeper: should pass after notarization

## Local Signing

### Option A: tauri.conf.json

Add the signing identity to `tauri-workspace/src-tauri/tauri.conf.json`:

```json
"bundle": {
  "macOS": {
    "signingIdentity": "Developer ID Application: Your Name (TEAMID)"
  }
}
```

### Option B: Environment variable

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
cargo tauri build
```

### Entitlements

The `Entitlements.plist` is already configured in `tauri.conf.json` under `bundle.macOS.entitlements`. It includes:

- `com.apple.security.cs.allow-jit` — required for WKWebView JavaScript execution
- `com.apple.security.cs.allow-unsigned-executable-memory` — required for JIT compilation
- `com.apple.security.cs.allow-dyld-environment-variables` — required for plugin loading

Standard keychain access works automatically for non-sandboxed signed apps — no `keychain-access-groups` entitlement needed.

### Manual signing + verify

```bash
codesign --deep --force --verify --verbose \
  --sign "Developer ID Application: Your Name (TEAMID)" \
  --options runtime \
  --entitlements tauri-workspace/src-tauri/Entitlements.plist \
  tauri-workspace/src-tauri/target/release/bundle/macos/NEARx.app

# Verify
codesign -dvv tauri-workspace/src-tauri/target/release/bundle/macos/NEARx.app
spctl --assess --type exec --verbose tauri-workspace/src-tauri/target/release/bundle/macos/NEARx.app
```

For local QA builds signed with `Apple Development`, `spctl` rejection is expected until the app is notarized or otherwise distributed through a trusted local workflow. The relevant biometric gate is code signing, not Gatekeeper acceptance.

## Notarization

### App Store Connect API Key (recommended)

1. Create an API key at [App Store Connect > Users and Access > Keys](https://appstoreconnect.apple.com/access/api)
2. Download the `.p8` file
3. Set environment variables:

```bash
export APPLE_API_ISSUER="your-issuer-id"
export APPLE_API_KEY="your-key-id"
export APPLE_API_KEY_PATH="/path/to/AuthKey_XXXX.p8"
```

Tauri will automatically notarize during `cargo tauri build` when these are set.

### Manual notarization

```bash
xcrun notarytool submit target/release/bundle/macos/NEARx.app.zip \
  --apple-id "your@email.com" \
  --team-id "TEAMID" \
  --password "app-specific-password" \
  --wait

xcrun stapler staple target/release/bundle/macos/NEARx.app
```

## CI Secrets

For GitHub Actions, add these repository secrets:

| Secret | Description |
|--------|-------------|
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the `.p12` file |
| `APPLE_SIGNING_IDENTITY` | Full identity string |
| `APPLE_API_ISSUER` | App Store Connect API issuer ID |
| `APPLE_API_KEY` | App Store Connect API key ID |
| `APPLE_API_KEY_PATH` | Content of the `.p8` key file |

### CI workflow snippet

```yaml
- name: Import certificate
  if: runner.os == 'macOS'
  env:
    APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
    APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
  run: |
    echo "$APPLE_CERTIFICATE" | base64 --decode > certificate.p12
    security create-keychain -p "" build.keychain
    security import certificate.p12 -k build.keychain -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
    security set-keychain-settings -t 3600 -u build.keychain
    security list-keychains -s build.keychain
    security set-key-partition-list -S apple-tool:,apple: -s -k "" build.keychain

- name: Build Tauri (signed + notarized)
  if: runner.os == 'macOS'
  env:
    APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
    APPLE_API_ISSUER: ${{ secrets.APPLE_API_ISSUER }}
    APPLE_API_KEY: ${{ secrets.APPLE_API_KEY }}
    APPLE_API_KEY_PATH: ${{ secrets.APPLE_API_KEY_PATH }}
  run: |
    cd tauri-workspace
    cargo tauri build
```

## Sidecar Signing

The nearxd sidecar binary bundled at `Contents/MacOS/nearxd-<triple>` is automatically signed with the same identity when `codesign --deep` is used. Tauri's bundler handles this when `signingIdentity` is configured.

## Version Alignment

The Tauri JS packages in `web/package.json` must stay pinned to versions compatible with the Rust crates in `tauri-workspace/Cargo.lock`. Avoid floating `^` ranges for:

- `@tauri-apps/api`
- `@tauri-apps/plugin-deep-link`

If `cargo tauri build` reports mismatched package versions, align the JS pins first and rerun `yarn install`.

## Fingerprint QA Gate

Fingerprint-protected Keychain signing must be validated on a properly signed macOS app bundle. Ad-hoc debug binaries and ad-hoc bundled `.app` artifacts can still read and write normal Keychain entries, but protected Keychain writes may fall back to unprotected storage and will not satisfy NEARx's biometric Keychain checks.

For local QA, an `Apple Development`-signed bundle is sufficient as long as both the app and bundled `nearxd` are non-adhoc signed. For release/distribution QA, require `Developer ID Application` plus notarization.

Before treating Touch ID / fingerprint behavior as a release result, verify:

```bash
codesign -dv --verbose=4 path/to/NEARx.app 2>&1 | rg 'Signature|TeamIdentifier'
codesign -dv --verbose=4 path/to/NEARx.app/Contents/MacOS/nearxd 2>&1 | rg 'Signature|TeamIdentifier'
```

Neither binary should report `Signature=adhoc`.

Local QA bundles should also report a real `TeamIdentifier`, for example:

```bash
codesign -dv --verbose=4 tauri-workspace/target/debug/bundle/macos/NEARx.app 2>&1 | rg 'Authority|TeamIdentifier|Signature'
codesign -dv --verbose=4 tauri-workspace/target/debug/bundle/macos/NEARx.app/Contents/MacOS/nearxd 2>&1 | rg 'Authority|TeamIdentifier|Signature'
```
