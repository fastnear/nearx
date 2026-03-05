#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "" ]]; then
  echo "usage: tools/release.sh <version>   # e.g. 0.9.0"
  exit 1
fi
VER="$1"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Formatting & linting"
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings

echo "==> Tests"
cargo test --workspace

echo "==> Preflight"
tools/preflight.sh

echo "==> Install JS dependencies"
corepack enable
yarn install

echo "==> Build explorer frontend"
yarn workspace explorer-frontend build

if [[ ! -d "web/dist" ]]; then
  echo "error: expected web/dist"
  exit 2
fi

echo "==> Build nearxd sidecar"
tools/build-sidecar.sh

echo "==> Tauri desktop build"
pushd tauri-workspace/src-tauri >/dev/null
cargo tauri build
popd >/dev/null

echo "==> Version/tag"
git add -A
git commit -m "NEARx v$VER: explorer frontend + Tauri integration"
git tag -s "v$VER" -m "NEARx v$VER"
git push origin HEAD --tags

echo "==> Done"
echo "Artifacts:"
echo "  - Tauri bundles under: tauri-workspace/src-tauri/target/release/bundle/"
echo "  - Web bundle under: web/dist/"
