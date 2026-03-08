#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="NEARx"
TAURI_DIR="$ROOT/tauri-workspace"
APP_PATH="$TAURI_DIR/target/debug/bundle/macos/${APP_NAME}.app"
SIDECAR_PATH="$APP_PATH/Contents/MacOS/nearxd"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: tools/build-macos-qa.sh must be run on macOS" >&2
  exit 1
fi

pick_identity() {
  if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    printf '%s\n' "$APPLE_SIGNING_IDENTITY"
    return 0
  fi

  local identities
  identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"

  local developer_id
  developer_id="$(printf '%s\n' "$identities" | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' | head -n 1)"
  if [[ -n "$developer_id" ]]; then
    printf '%s\n' "$developer_id"
    return 0
  fi

  local apple_development
  apple_development="$(printf '%s\n' "$identities" | sed -n 's/.*"\(Apple Development:.*\)"/\1/p' | head -n 1)"
  if [[ -n "$apple_development" ]]; then
    printf '%s\n' "$apple_development"
    return 0
  fi

  return 1
}

verify_signed_binary() {
  local path="$1"
  local label="$2"
  local details
  details="$(codesign -dv --verbose=4 "$path" 2>&1)"
  printf '%s\n' "$details" | rg -q 'Signature=adhoc' && {
    echo "error: $label is still ad-hoc signed: $path" >&2
    printf '%s\n' "$details" >&2
    exit 2
  }
  printf '%s\n' "$details" | rg -q 'TeamIdentifier=' || {
    echo "error: $label is missing TeamIdentifier: $path" >&2
    printf '%s\n' "$details" >&2
    exit 3
  }
}

IDENTITY="$(pick_identity || true)"
if [[ -z "$IDENTITY" ]]; then
  echo "error: no usable codesigning identity found." >&2
  echo "Install a 'Developer ID Application' certificate for release-style QA, or an 'Apple Development' certificate for local signed QA." >&2
  exit 4
fi

echo "==> Using signing identity"
echo "    $IDENTITY"

echo "==> Install JS dependencies"
corepack enable >/dev/null 2>&1 || true
yarn install --mode=skip-build

echo "==> Build web frontend"
npm --prefix "$ROOT/web" run build

echo "==> Build nearxd sidecar"
node "$ROOT/tools/build-sidecar.mjs"

echo "==> Build signed Tauri app bundle"
(
  cd "$TAURI_DIR"
  APPLE_SIGNING_IDENTITY="$IDENTITY" cargo tauri build --debug --bundles app
)

if [[ ! -d "$APP_PATH" ]]; then
  echo "error: expected app bundle at $APP_PATH" >&2
  exit 5
fi

if [[ ! -x "$SIDECAR_PATH" ]]; then
  echo "error: expected bundled nearxd at $SIDECAR_PATH" >&2
  exit 6
fi

echo "==> Verify signatures"
verify_signed_binary "$APP_PATH" "app bundle"
verify_signed_binary "$SIDECAR_PATH" "nearxd sidecar"

echo "==> Verify Gatekeeper assessment"
spctl --assess --type exec --verbose "$APP_PATH" || true

echo "==> Signed QA bundle ready"
echo "App:     $APP_PATH"
echo "Sidecar: $SIDECAR_PATH"
