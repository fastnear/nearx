#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE=$(rustc --print host-tuple)
SIDECAR_DIR="$ROOT/tauri-workspace/src-tauri/binaries"
mkdir -p "$SIDECAR_DIR"
cargo build --release --bin nearxd --manifest-path "$ROOT/Cargo.toml"
install -m 755 "$ROOT/target/release/nearxd" "$SIDECAR_DIR/nearxd-$TARGET_TRIPLE"
echo "Sidecar: $SIDECAR_DIR/nearxd-$TARGET_TRIPLE"
