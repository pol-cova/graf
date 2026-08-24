#!/usr/bin/env bash
set -euo pipefail

# Graf macOS .app Bundle and DMG Packaging Script (Milestone M7)

APP_NAME="Graf"
BUNDLE_DIR="target/release/bundle/${APP_NAME}.app"
CONTENTS_DIR="${BUNDLE_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "Building release binary for aarch64-apple-darwin..."
cargo build --release

echo "Creating macOS application bundle at ${BUNDLE_DIR}..."
rm -rf "target/release/bundle"
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

# Copy executable binary
cp "target/release/Graf" "${MACOS_DIR}/Graf"
chmod +x "${MACOS_DIR}/Graf"

# Copy Info.plist
cp "bundle/Info.plist" "${CONTENTS_DIR}/Info.plist"

echo "${APP_NAME}.app created at ${BUNDLE_DIR}."

if command -v hdiutil &>/dev/null; then
    DMG_PATH="target/release/Graf-v0.1.0-aarch64.dmg"
    echo "Creating distribution DMG at ${DMG_PATH}..."
    hdiutil create -volname "${APP_NAME}" -srcfolder "${BUNDLE_DIR}" -ov -format UDZO "${DMG_PATH}"
    echo "DMG created at ${DMG_PATH}."
fi
