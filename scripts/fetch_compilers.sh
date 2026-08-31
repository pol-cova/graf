#!/usr/bin/env bash
set -euo pipefail

# Fetch the compiler backends pinned in COMPILERS.lock into
# vendor/compilers/bin for scripts/build_app.sh to copy into graf.app.
# Artifacts with a matching hash are kept, so re-running is cheap.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK_FILE="${REPO_ROOT}/COMPILERS.lock"
CACHE_DIR="${REPO_ROOT}/vendor/compilers/cache"
BIN_DIR="${REPO_ROOT}/vendor/compilers/bin"

ARCH="$(uname -m)"
if [[ "${ARCH}" != "arm64" ]]; then
    echo "error: COMPILERS.lock pins aarch64-apple-darwin artifacts only, cannot bundle for ${ARCH}" >&2
    exit 1
fi

mkdir -p "${CACHE_DIR}" "${BIN_DIR}"

# Emit one line per lockfile section: name, url, sha256, inner path.
lock_entries() {
    awk '
        /^\[/ {
            if (name != "") print name "\t" url "\t" sha256 "\t" inner
            name = substr($1, 2, length($1) - 2)
            url = ""; sha256 = ""; inner = ""
            next
        }
        /^url[ \t]*=/ { sub(/^[^=]*=[ \t]*/, ""); url = $0 }
        /^sha256[ \t]*=/ { sub(/^[^=]*=[ \t]*/, ""); sha256 = $0 }
        /^inner[ \t]*=/ { sub(/^[^=]*=[ \t]*/, ""); inner = $0 }
        END { if (name != "") print name "\t" url "\t" sha256 "\t" inner }
    ' "${LOCK_FILE}"
}

while IFS=$'\t' read -r name url sha256 inner; do
    artifact="${CACHE_DIR}/${name}.download"

    if [[ -f "${artifact}" ]] \
        && [[ "$(shasum -a 256 "${artifact}" | awk '{print $1}')" == "${sha256}" ]]; then
        echo "${name}: artifact already verified"
    else
        echo "${name}: downloading ${url}"
        curl -sSfL --retry 3 -o "${artifact}" "${url}"
        actual="$(shasum -a 256 "${artifact}" | awk '{print $1}')"
        if [[ "${actual}" != "${sha256}" ]]; then
            echo "error: ${name} hash mismatch (expected ${sha256}, got ${actual})" >&2
            exit 1
        fi
    fi

    tar -xf "${artifact}" -C "${CACHE_DIR}" "${inner}"
    cp "${CACHE_DIR}/${inner}" "${BIN_DIR}/${name}"
    chmod +x "${BIN_DIR}/${name}"
    version="$("${BIN_DIR}/${name}" --version 2>/dev/null | head -n 1 || true)"
    echo "${name}: installed (${version:-version check failed})"
done < <(lock_entries)
