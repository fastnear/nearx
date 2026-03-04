#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TARGET_DIR="$ROOT/target/wasm32-unknown-unknown/debug"
OUT_DIR="$ROOT/web/pkg"

echo "[web] Building nearx-web-dom (wasm32-unknown-unknown, debug)..."
cargo build \
  --manifest-path "$ROOT/Cargo.toml" \
  --bin nearx-web-dom \
  --no-default-features \
  --features dom-web \
  --target wasm32-unknown-unknown

mkdir -p "$OUT_DIR"

echo "[web] Running wasm-bindgen..."
wasm-bindgen \
  --target web \
  --no-typescript \
  --out-dir "$OUT_DIR" \
  --out-name nearx_web_dom \
  "$TARGET_DIR/nearx-web-dom.wasm"

echo "[web] Done. Serve ./web with any static server (or Tauri)."
