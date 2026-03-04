#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "[web-dev] Building..."
"$ROOT/tools/build-web.sh"

echo "[web-dev] Starting dev server on http://localhost:4173..."
cd "$ROOT/web"
python3 -m http.server 4173
