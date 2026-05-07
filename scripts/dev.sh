#!/usr/bin/env bash
# Linux dev shell for scryd, runs in a Debian-based Rust container.
# Repo PWD mounts at /workspace. target/ and cargo registry use named
# Docker volumes so the Linux toolchain doesn't clobber any host
# build state and so dependencies cache across runs.
#
# Usage:
#   ./scripts/dev.sh                       # interactive bash shell
#   ./scripts/dev.sh cargo check --workspace
#   ./scripts/dev.sh cargo test -p scryd-config

set -euo pipefail

image="rust:1-bookworm"
workspace="$(pwd)"
target_vol="mail-clawd-target"
cargo_vol="mail-clawd-cargo"

docker volume create "$target_vol" >/dev/null
docker volume create "$cargo_vol"  >/dev/null

base_args=(
    run --rm
    -v "${workspace}:/workspace"
    -v "${target_vol}:/workspace/target"
    -v "${cargo_vol}:/usr/local/cargo/registry"
    -w /workspace
    -e CARGO_HOME=/usr/local/cargo
    -e CARGO_TARGET_DIR=/workspace/target
)

if [[ $# -eq 0 ]]; then
    exec docker "${base_args[@]}" -it "$image" bash
else
    exec docker "${base_args[@]}" "$image" "$@"
fi
