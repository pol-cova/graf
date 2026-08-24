#!/usr/bin/env bash
set -euo pipefail

if ! command -v xctrace >/dev/null 2>&1; then
    echo "xctrace is required. Install Xcode and its command-line tools." >&2
    exit 1
fi

output="${1:-graf-time-profile-$(date +%Y%m%d-%H%M%S).trace}"

cargo build --profile profiling
xcrun xctrace record \
    --template "Time Profiler" \
    --output "$output" \
    --launch -- "$(pwd)/target/profiling/Graf"

echo "Profile saved to $output"
