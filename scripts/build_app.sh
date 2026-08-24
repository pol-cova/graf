#!/usr/bin/env bash
set -euo pipefail


APP_NAME="graf"
VERSION="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
HOST_TRIPLE="$(rustc -vV | awk '/^host:/ { print $2 }')"
ARCH="$(uname -m)"
BUNDLE_DIR="target/release/bundle/${APP_NAME}.app"
CONTENTS_DIR="${BUNDLE_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "Building graf ${VERSION} for ${HOST_TRIPLE}..."
cargo build --release

echo "Creating macOS application bundle at ${BUNDLE_DIR}..."
rm -rf "target/release/bundle"
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

cp "target/release/graf" "${MACOS_DIR}/graf"
chmod +x "${MACOS_DIR}/graf"

cp "bundle/Info.plist" "${CONTENTS_DIR}/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string ${VERSION}" "${CONTENTS_DIR}/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleVersion string ${VERSION}" "${CONTENTS_DIR}/Info.plist"

if [[ -n "${GRAF_CODESIGN_IDENTITY:-}" ]]; then
    echo "Signing ${APP_NAME}.app with ${GRAF_CODESIGN_IDENTITY}..."
    codesign --force --deep --options runtime --timestamp \
        --sign "${GRAF_CODESIGN_IDENTITY}" "${BUNDLE_DIR}"
    codesign --verify --deep --strict --verbose=2 "${BUNDLE_DIR}"
fi

echo "${APP_NAME}.app created at ${BUNDLE_DIR}."

if command -v hdiutil &>/dev/null; then
    DMG_PATH="target/release/graf-v${VERSION}-${ARCH}.dmg"
    echo "Creating distribution DMG at ${DMG_PATH}..."
    hdiutil create -volname "${APP_NAME}" -srcfolder "${BUNDLE_DIR}" -ov -format UDZO "${DMG_PATH}"

    if [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
        echo "Submitting ${APP_NAME} for notarization..."
        xcrun notarytool submit "${DMG_PATH}" \
            --apple-id "${APPLE_ID}" \
            --team-id "${APPLE_TEAM_ID}" \
            --password "${APPLE_APP_PASSWORD}" \
            --wait
        xcrun stapler staple "${DMG_PATH}"
        xcrun stapler validate "${DMG_PATH}"
    fi

    echo "DMG created at ${DMG_PATH}."
fi
