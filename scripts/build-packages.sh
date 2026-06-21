#!/usr/bin/env bash
# Build scryd's native Linux packages (deb, rpm, apk, archlinux) with nfpm.
#
# Stages the release binaries, renders the systemd unit for the packaged binary
# path, and runs `nfpm package` once per format. Intended to run after
# `cargo build --release --target <triple> -p scryd -p scryd-fetch-weights`.
#
# Environment:
#   ARCH      nfpm architecture: amd64 | arm64               (required)
#   TARGET    Rust target triple the binaries were built for (required)
#   VERSION   package version (semver). Derived when empty:
#               a pushed v* tag wins, else the workspace Cargo.toml version.
#   OUTDIR    where to write packages          (default: dist/packages)
#   FORMATS   space-separated nfpm packagers   (default: "deb rpm apk archlinux")
#   NFPM      nfpm binary to invoke            (default: nfpm)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ARCH="${ARCH:?ARCH is required (amd64|arm64)}"
TARGET="${TARGET:?TARGET is required (a Rust target triple)}"
OUTDIR="${OUTDIR:-dist/packages}"
FORMATS="${FORMATS:-deb rpm apk archlinux}"
NFPM="${NFPM:-nfpm}"
VERSION="${VERSION:-}"

# Derive the version when the caller didn't pin one.
if [[ -z "$VERSION" ]]; then
    if [[ "${GITHUB_REF_TYPE:-}" == "tag" && "${GITHUB_REF_NAME:-}" == v* ]]; then
        VERSION="${GITHUB_REF_NAME#v}"
    else
        VERSION="$(grep -m1 -E '^version[[:space:]]*=' Cargo.toml \
            | sed -E 's/.*"([^"]+)".*/\1/')"
    fi
fi
if [[ -z "$VERSION" ]]; then
    echo "build-packages: could not determine VERSION" >&2
    exit 1
fi

bindir="target/${TARGET}/release"
for f in scryd scryd-fetch-weights; do
    if [[ ! -f "$bindir/$f" ]]; then
        echo "build-packages: missing $bindir/$f — build the release target first" >&2
        exit 1
    fi
done

stage="dist/stage"
rm -rf "$stage"
mkdir -p "$stage"

install -m 0755 "$bindir/scryd" "$stage/scryd"
install -m 0755 "$bindir/scryd-fetch-weights" "$stage/scryd-fetch-weights"

# Packages install the binary at /usr/bin (they own /usr), while the manual
# tarball installer uses /usr/local/bin. Render the unit to match, keeping
# ops/scryd.service.in the single source of truth for everything else.
sed 's#/usr/local/bin/scryd#/usr/bin/scryd#g' ops/scryd.service.in > "$stage/scryd.service"

mkdir -p "$OUTDIR"

export PKG_ARCH="$ARCH"
export PKG_VERSION="$VERSION"

echo "build-packages: scryd ${VERSION} (${ARCH}, ${TARGET}) -> ${OUTDIR}"
for fmt in $FORMATS; do
    echo "  building ${fmt}"
    "$NFPM" package --config ops/nfpm/nfpm.yaml --packager "$fmt" --target "$OUTDIR"
done

echo "build-packages: produced"
ls -1 "$OUTDIR"
