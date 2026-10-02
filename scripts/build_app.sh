#!/usr/bin/env bash
# Builds Graf.app: the Rust core as an XCFramework, the Swift app on top,
# and the bundled Tectonic and Typst backends. Optionally signs, packages a
# DMG, and notarizes.
#
#   scripts/build_app.sh              release build, DMG
#   GRAF_CONFIGURATION=debug scripts/build_app.sh --no-dmg
#   GRAF_SKIP_COMPILERS=1 ...         use system Tectonic/Typst instead
#
# Signing and notarization run when these are set:
#   GRAF_CODESIGN_IDENTITY, APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD
set -euo pipefail

cd "$(dirname "$0")/.."

CONFIGURATION="${GRAF_CONFIGURATION:-release}"
MAKE_DMG=1
[[ "${1:-}" == "--no-dmg" ]] && MAKE_DMG=0

# The version gate names the crate instead of taking packages[0]. This is a
# workspace now, so the first entry is whichever member Cargo happens to
# order first (currently graf-core) and would silently compare the wrong
# crate if any member declared an explicit version. Fails loudly if the
# crate is missing or the version is absent.
VERSION="$(cargo metadata --no-deps --format-version 1 \
    | python3 -c '
import json, sys
packages = json.load(sys.stdin)["packages"]
matches = [p for p in packages if p["name"] == "graf-core"]
if not matches:
    sys.exit("graf-core not found in the cargo workspace")
version = matches[0].get("version")
if not version:
    sys.exit("graf-core has no version")
print(version)
')"
ARCH="$(uname -m)"
BUNDLE_DIR="target/${CONFIGURATION}/bundle/Graf.app"
CONTENTS_DIR="${BUNDLE_DIR}/Contents"

echo "Building the Rust core (${CONFIGURATION})..."
if [[ "${CONFIGURATION}" == "release" ]]; then
    ./scripts/build_core_xcframework.sh
else
    GRAF_CORE_PROFILE=dev ./scripts/build_core_xcframework.sh
fi

echo "Building the Swift app (${CONFIGURATION})..."
swift build --package-path apple -c "${CONFIGURATION}" --product Graf
BIN_DIR="$(swift build --package-path apple -c "${CONFIGURATION}" --show-bin-path)"

echo "Assembling ${BUNDLE_DIR}..."
rm -rf "${BUNDLE_DIR}"
mkdir -p "${CONTENTS_DIR}/MacOS" "${CONTENTS_DIR}/Resources"
cp "${BIN_DIR}/Graf" "${CONTENTS_DIR}/MacOS/Graf"
cp "bundle/Info.plist" "${CONTENTS_DIR}/Info.plist"
cp "bundle/AppIcon.icns" "${CONTENTS_DIR}/Resources/AppIcon.icns"
/usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string ${VERSION}" "${CONTENTS_DIR}/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleVersion string ${VERSION}" "${CONTENTS_DIR}/Info.plist"

if [[ -z "${GRAF_SKIP_COMPILERS:-}" ]]; then
    COMPILERS_BIN="vendor/compilers/bin"
    if [[ ! -x "${COMPILERS_BIN}/tectonic" || ! -x "${COMPILERS_BIN}/typst" ]]; then
        echo "Fetching bundled compiler backends..."
        bash scripts/fetch_compilers.sh
    fi
    # graf-core looks for bundled engines in Contents/Resources/bin.
    mkdir -p "${CONTENTS_DIR}/Resources/bin"
    cp "${COMPILERS_BIN}/tectonic" "${COMPILERS_BIN}/typst" "${CONTENTS_DIR}/Resources/bin/"
    chmod +x "${CONTENTS_DIR}/Resources/bin/tectonic" "${CONTENTS_DIR}/Resources/bin/typst"
    cp -R "bundle/licenses" "${CONTENTS_DIR}/Resources/bin/LICENSES"
fi

if [[ -n "${GRAF_CODESIGN_IDENTITY:-}" ]]; then
    echo "Signing with ${GRAF_CODESIGN_IDENTITY}..."
    if [[ -d "${CONTENTS_DIR}/Resources/bin" ]]; then
        codesign --force --options runtime --timestamp --sign "${GRAF_CODESIGN_IDENTITY}" \
            "${CONTENTS_DIR}/Resources/bin/tectonic" "${CONTENTS_DIR}/Resources/bin/typst"
    fi
    codesign --force --options runtime --timestamp --sign "${GRAF_CODESIGN_IDENTITY}" "${BUNDLE_DIR}"
    codesign --verify --deep --strict --verbose=2 "${BUNDLE_DIR}"
else
    # Ad-hoc signature so the app runs locally on Apple Silicon.
    codesign --force --sign - "${BUNDLE_DIR}"
fi

echo "Graf.app created at ${BUNDLE_DIR}."

if [[ "${MAKE_DMG}" == 1 ]] && command -v hdiutil &>/dev/null; then
    DMG_PATH="target/${CONFIGURATION}/graf-v${VERSION}-${ARCH}.dmg"
    echo "Creating ${DMG_PATH}..."
    hdiutil create -volname "graf" -srcfolder "${BUNDLE_DIR}" -ov -format UDZO "${DMG_PATH}" >/dev/null

    if [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
        echo "Notarizing..."
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
