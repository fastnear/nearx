#!/usr/bin/env bash
set -euo pipefail

SOCK="${NEARXD_SOCKET_PATH:-/tmp/nearxd.sock}"
NETWORK="${1:-testnet}"
CREDS_DIR="${2:-$HOME/.near-credentials/$NETWORK}"
ACCOUNT_ID="${3:-}"

if ! command -v nc >/dev/null 2>&1; then
  echo "error: nc (netcat) is required" >&2
  exit 1
fi

if [[ ! -S "$SOCK" ]]; then
  echo "error: nearxd socket not found at $SOCK" >&2
  echo "hint: run nearxd with NEARXD_SOCKET_PATH=$SOCK" >&2
  exit 1
fi

rpc() {
  local id="$1"
  local method="$2"
  local params="$3"
  printf '{"id":"%s","method":"%s","params":%s}\n' "$id" "$method" "$params" | nc -U "$SOCK"
  echo
}

echo "== probe_user_presence =="
rpc "1" "probe_user_presence" '{"allow_fallback":true}'

echo "== request_user_presence (expect Touch ID prompt) =="
rpc "2" "request_user_presence" '{"reason":"NEARx QA: approve local signing test","allow_fallback":false}'

echo "== import_near_credentials ($CREDS_DIR) =="
rpc "3" "import_near_credentials" "{\"network\":\"$NETWORK\",\"credentials_dir\":\"$CREDS_DIR\",\"require_user_presence\":true,\"allow_fallback\":false,\"persist_in_keychain\":true,\"keychain_credential_protection\":\"biometry_current_set\",\"save_settings\":true}"

if [[ -n "$ACCOUNT_ID" ]]; then
  echo "== get_near_credential ($NETWORK:$ACCOUNT_ID) =="
  rpc "4" "get_near_credential" "{\"network\":\"$NETWORK\",\"account_id\":\"$ACCOUNT_ID\",\"reason\":\"NEARx QA: read imported credential\"}"
fi
