#!/bin/bash

# Simple archive script for non-git projects
# Creates: nearx-archive.zip

set -e

PROJECT_NAME="nearx"
ARCHIVE_NAME="${PROJECT_NAME}-archive.zip"

echo "Creating archive of ${PROJECT_NAME}..."

# Remove old archive if it exists
[ -f "${ARCHIVE_NAME}" ] && rm "${ARCHIVE_NAME}"

# Create archive excluding common build artifacts and temporary files
zip -r "${ARCHIVE_NAME}" . \
    -x "target/*" \
    -x "nearx_debug.log" \
    -x "*.zip" \
    -x ".DS_Store" \
    -x ".idea/*" \
    -x "Cargo.lock" \
    -x ".claude/*" \
    -x "__pycache__/*" \
    -x "*.pyc" \
    -x ".git/*" \
    -x ".gitignore" \
    -x "extension/*" \
    -x "native-host/*" \
    -x "tauri-workspace/*" \
    -x "dist/*" \
    -x "dist-dom/*" \
    -x "dist-egui/*" \
    -x "node_modules/*" \
    -x "*/node_modules/*" \
    -x "web/pkg/*" \
    -x ".env" \
    -x "Trunk.toml" \
    -x "index.html" \
    -x "debug.log" \
    -x "*.db" \
    -x "*.db-shm" \
    -x "*.db-wal" \
    -x "BROWSER_EXTENSION_SETUP.md" \
    -x "IMPLEMENTATION_SUMMARY.md"

# Display archive info
if [ -f "${ARCHIVE_NAME}" ]; then
    SIZE=$(du -h "${ARCHIVE_NAME}" | cut -f1)
    FILE_COUNT=$(unzip -l "${ARCHIVE_NAME}" | grep -E "^\s*[0-9]+" | wc -l | tr -d ' ')

    echo ""
    echo "✓ Archive created successfully!"
    echo "  File: ${ARCHIVE_NAME}"
    echo "  Size: ${SIZE}"
    echo "  Files: ${FILE_COUNT}"
    echo ""
    echo "Archive contents:"
    unzip -l "${ARCHIVE_NAME}"
else
    echo "Error: Failed to create archive"
    exit 1
fi
