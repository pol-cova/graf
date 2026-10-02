#!/usr/bin/env bash
# Builds graf-ffi as a static library, generates its Swift bindings, and
# places both where the Swift package in apple/ expects them:
#   apple/Frameworks/GrafCore.xcframework    static library, C header, modulemap
#   apple/Sources/GrafCore/graf_ffi.swift     generated Swift API
# Both outputs are build products and are not tracked.
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="${GRAF_CORE_PROFILE:-release}"
TARGET="aarch64-apple-darwin"
STAGING="target/apple"
FRAMEWORK="apple/Frameworks/GrafCore.xcframework"
SWIFT_DIR="apple/Sources/GrafCore"
# Cargo's dev profile writes to target/<triple>/debug.
PROFILE_DIR="${PROFILE}"
[[ "${PROFILE}" == "dev" ]] && PROFILE_DIR="debug"
LIB="target/${TARGET}/${PROFILE_DIR}/libgraf_ffi.a"

if [[ "${PROFILE}" == "release" ]]; then
    cargo build -p graf-ffi --release --target "${TARGET}"
else
    cargo build -p graf-ffi --profile "${PROFILE}" --target "${TARGET}"
fi

rm -rf "${STAGING}" "${FRAMEWORK}"
mkdir -p "${STAGING}/Headers" "${SWIFT_DIR}" "$(dirname "${FRAMEWORK}")"

bindgen() {
    cargo run --quiet -p uniffi-bindgen-swift -- "${LIB}" "$@"
}

bindgen "${SWIFT_DIR}" --swift-sources
bindgen "${STAGING}/Headers" --headers
# A plain (non-framework) module, named the way the generated Swift imports
# it, because the XCFramework wraps a static library with loose headers.
bindgen "${STAGING}/Headers" --modulemap --module-name graf_ffiFFI --modulemap-filename module.modulemap

xcodebuild -create-xcframework \
    -library "${LIB}" \
    -headers "${STAGING}/Headers" \
    -output "${FRAMEWORK}" >/dev/null

echo "Built ${FRAMEWORK} and ${SWIFT_DIR}/graf_ffi.swift"
