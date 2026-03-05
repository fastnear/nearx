#!/usr/bin/env bash
#
# tools/sync-explorer.sh — check web/ for divergence from upstream explorer-frontend
#
# Usage:
#   tools/sync-explorer.sh              # compare against last synced commit
#   tools/sync-explorer.sh --latest     # compare against upstream HEAD
#   tools/sync-explorer.sh --diff       # show full diffs (not just summary)
#   tools/sync-explorer.sh --apply      # interactively apply upstream changes
#   tools/sync-explorer.sh --bump       # update synced_sha to upstream HEAD
#
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="$ROOT/web/.explorer-upstream.json"

if ! command -v gh &>/dev/null; then
  echo "error: gh CLI required (brew install gh)" >&2
  exit 1
fi
if ! command -v jq &>/dev/null; then
  echo "error: jq required (brew install jq)" >&2
  exit 1
fi

REPO=$(jq -r .repo "$CONFIG")
BRANCH=$(jq -r .branch "$CONFIG")
SYNCED_SHA=$(jq -r .synced_sha "$CONFIG")
LOCAL_PREFIX=$(jq -r .local_prefix "$CONFIG")

# Parse NEARx-only paths into an array (bash 3 / zsh compatible)
NEARX_ONLY=()
while IFS= read -r line; do
  NEARX_ONLY+=("$line")
done < <(jq -r '.nearx_only[]' "$CONFIG")

MODE="summary"
REF="$SYNCED_SHA"

for arg in "$@"; do
  case "$arg" in
    --latest)  REF="$BRANCH" ;;
    --diff)    MODE="diff" ;;
    --apply)   MODE="apply" ;;
    --bump)    MODE="bump" ;;
    --help|-h)
      echo "Usage: tools/sync-explorer.sh [--latest] [--diff] [--apply] [--bump]"
      exit 0 ;;
  esac
done

# Resolve the actual SHA for display
if [ "$REF" = "$BRANCH" ]; then
  COMPARE_SHA=$(gh api "repos/$REPO/commits/$BRANCH" --jq '.sha' 2>/dev/null)
else
  COMPARE_SHA="$REF"
fi

echo "Upstream:  $REPO @ ${COMPARE_SHA:0:7}"
echo "Synced at: ${SYNCED_SHA:0:7}"
echo "Local:     $LOCAL_PREFIX/"
echo ""

if [ "$MODE" = "bump" ]; then
  HEAD_SHA=$(gh api "repos/$REPO/commits/$BRANCH" --jq '.sha' 2>/dev/null)
  HEAD_DATE=$(gh api "repos/$REPO/commits/$BRANCH" --jq '.commit.author.date | split("T")[0]' 2>/dev/null)
  jq --arg sha "$HEAD_SHA" --arg date "$HEAD_DATE" \
    '.synced_sha = $sha | .synced_date = $date' "$CONFIG" > "$CONFIG.tmp"
  mv "$CONFIG.tmp" "$CONFIG"
  echo "Updated synced_sha to $HEAD_SHA ($HEAD_DATE)"
  exit 0
fi

# Check for new upstream commits since last sync
if [ "$REF" = "$SYNCED_SHA" ]; then
  AHEAD=$(gh api "repos/$REPO/compare/${SYNCED_SHA}...${BRANCH}" --jq '.ahead_by' 2>/dev/null || echo "?")
  if [ "$AHEAD" = "0" ]; then
    echo "Upstream has no new commits since last sync."
    echo ""
  elif [ "$AHEAD" != "?" ]; then
    echo "Upstream has $AHEAD new commit(s) since last sync. Run with --latest to compare."
    echo ""
  fi
fi

is_nearx_only() {
  local path="$1"
  for prefix in "${NEARX_ONLY[@]}"; do
    # Directory prefix (ends with /)
    if [[ "$prefix" == */ ]] && [[ "$path" == "$prefix"* ]]; then
      return 0
    fi
    # Exact file match
    if [ "$path" = "$prefix" ]; then
      return 0
    fi
  done
  return 1
}

# List all files in the upstream tree at the given ref
UPSTREAM_FILES=$(gh api "repos/$REPO/git/trees/$COMPARE_SHA?recursive=1" \
  --jq '.tree[] | select(.type == "blob") | .path' 2>/dev/null)

SAME=0
DIVERGED=()
UPSTREAM_ONLY=()
LOCAL_ONLY_SHARED=()
TMPDIR_SYNC=$(mktemp -d)
trap 'rm -rf "$TMPDIR_SYNC"' EXIT

# Compare each upstream file against local
while IFS= read -r upath; do
  local_path="$ROOT/$LOCAL_PREFIX/$upath"

  if [ ! -f "$local_path" ]; then
    UPSTREAM_ONLY+=("$upath")
    continue
  fi

  # Fetch upstream content, compare
  upstream_content=$(gh api "repos/$REPO/contents/$upath?ref=$COMPARE_SHA" --jq '.content' 2>/dev/null | base64 -d 2>/dev/null) || continue
  local_content=$(cat "$local_path")

  if [ "$upstream_content" = "$local_content" ]; then
    SAME=$((SAME + 1))
  else
    DIVERGED+=("$upath")
    if [ "$MODE" = "diff" ] || [ "$MODE" = "apply" ]; then
      echo "$upstream_content" > "$TMPDIR_SYNC/$( echo "$upath" | tr '/' '_')"
    fi
  fi
done <<< "$UPSTREAM_FILES"

# Check for local files that don't exist upstream (excluding NEARx-only)
while IFS= read -r local_file; do
  rel_path="${local_file#$ROOT/$LOCAL_PREFIX/}"
  # Skip non-source files
  [[ "$rel_path" == node_modules/* ]] && continue
  [[ "$rel_path" == dist/* ]] && continue
  [[ "$rel_path" == .* ]] && continue

  if is_nearx_only "$rel_path"; then
    continue
  fi

  # Check if this file exists in upstream
  if ! echo "$UPSTREAM_FILES" | grep -qxF "$rel_path"; then
    LOCAL_ONLY_SHARED+=("$rel_path")
  fi
done < <(find "$ROOT/$LOCAL_PREFIX/src" "$ROOT/$LOCAL_PREFIX/public" -type f 2>/dev/null)

# Report
echo "=== Sync Report ==="
echo ""
echo "Identical:      $SAME files"
echo "Diverged:       ${#DIVERGED[@]} files"
echo "Upstream only:  ${#UPSTREAM_ONLY[@]} files"
echo "Local only:     ${#LOCAL_ONLY_SHARED[@]} files (excluding NEARx-only)"
echo ""

if [ ${#DIVERGED[@]} -gt 0 ]; then
  echo "--- Diverged files ---"
  for f in "${DIVERGED[@]}"; do
    echo "  $f"
  done
  echo ""
fi

if [ ${#UPSTREAM_ONLY[@]} -gt 0 ]; then
  echo "--- Upstream only (missing locally) ---"
  for f in "${UPSTREAM_ONLY[@]}"; do
    echo "  $f"
  done
  echo ""
fi

if [ ${#LOCAL_ONLY_SHARED[@]} -gt 0 ]; then
  echo "--- Local only (not in upstream, not NEARx-only) ---"
  for f in "${LOCAL_ONLY_SHARED[@]}"; do
    echo "  $f"
  done
  echo ""
fi

# Show diffs
if [ "$MODE" = "diff" ] && [ ${#DIVERGED[@]} -gt 0 ]; then
  echo "=== Diffs (upstream -> local) ==="
  echo ""
  for f in "${DIVERGED[@]}"; do
    tmpfile="$TMPDIR_SYNC/$( echo "$f" | tr '/' '_')"
    if [ -f "$tmpfile" ]; then
      echo "--- $f ---"
      diff -u "$tmpfile" "$ROOT/$LOCAL_PREFIX/$f" \
        --label "upstream:$f" --label "local:$LOCAL_PREFIX/$f" || true
      echo ""
    fi
  done
fi

# Interactive apply
if [ "$MODE" = "apply" ] && [ ${#DIVERGED[@]} -gt 0 ]; then
  echo "=== Interactive apply ==="
  echo ""
  for f in "${DIVERGED[@]}"; do
    tmpfile="$TMPDIR_SYNC/$( echo "$f" | tr '/' '_')"
    if [ ! -f "$tmpfile" ]; then continue; fi

    echo "--- $f ---"
    diff -u "$tmpfile" "$ROOT/$LOCAL_PREFIX/$f" \
      --label "upstream:$f" --label "local:$LOCAL_PREFIX/$f" || true
    echo ""
    read -rp "  Overwrite local with upstream? [y/N/q] " choice
    case "$choice" in
      y|Y) cp "$tmpfile" "$ROOT/$LOCAL_PREFIX/$f"; echo "  -> applied" ;;
      q|Q) echo "  -> quit"; break ;;
      *)   echo "  -> skipped" ;;
    esac
    echo ""
  done
fi
