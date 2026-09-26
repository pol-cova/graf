#!/usr/bin/env bash
# Builds graf-ffi as a static library, generates its Swift bindings, and
# packages both as GrafCore.xcframework for the Xcode app.
#
# Output (all under target/apple, which is not tracked):
#   GrafCore.xcframework   static library + C headers + modulemap
#   Sources/graf_ffi.swift generated Swift API to compile into the app
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="${GRAF_CORE_PROFILE:-release}"
TARGET="aarch64-apple-darwin"
OUT="target/apple"
LIB="target/${TARGET}/${PROFILE}/libgraf_ffi.a"

if [[ "${PROFILE}" == "release" ]]; then
    cargo build -p graf-ffi --release --target "${TARGET}"
else
    cargo build -p graf-ffi --profile "${PROFILE}" --target "${TARGET}"
fi

rm -rf "${OUT}"
mkdir -p "${OUT}/Headers" "${OUT}/Sources"

bindgen() {
    cargo run --quiet -p uniffi-bindgen-swift -- "${LIB}" "$@"
}

bindgen "${OUT}/Sources" --swift-sources
bindgen "${OUT}/Headers" --headers
# A plain (non-framework) module, named the way the generated Swift imports
# it, because the XCFramework wraps a static library with loose headers.
bindgen "${OUT}/Headers" --modulemap --module-name graf_ffiFFI --modulemap-filename module.modulemap

xcodebuild -create-xcframework \
    -library "${LIB}" \
    -headers "${OUT}/Headers" \
    -output "${OUT}/GrafCore.xcframework" >/dev/null

echo "Built ${OUT}/GrafCore.xcframework and ${OUT}/Sources"
