#!/usr/bin/env bash
# Install scryd on the smoke VM. Builds from source on the VM itself —
# the Windows workstation can't easily cross-compile glibc binaries
# and shipping a release tarball makes the smoke loop slow when you're
# iterating on a local branch. Trade ~5 min of compile time for
# accuracy and no extra moving parts.
#
# Idempotent on the VM. Re-running rebuilds against whatever's
# currently checked in on the workstation.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

need_cmd tar
load_vm

REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
remote_src="/home/$SMOKE_VM_USER/scryd-src"

# Ship the working tree over SSH with `tar | ssh tar -x`. Portable
# across Git Bash / WSL / Linux — no rsync dependency. We re-create
# the directory each run so deletions on the workstation propagate
# (functionally equivalent to `rsync --delete`).
log "shipping source -> $SMOKE_VM_IP:$remote_src (tar pipe; excludes target/, .git/, .state/)"
ssh "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP" "rm -rf '$remote_src' && mkdir -p '$remote_src'"
tar -C "$REPO_ROOT" \
    --exclude='./target' \
    --exclude='./.git' \
    --exclude='./smoke-test/.state' \
    --exclude='./node_modules' \
    -cf - . \
  | ssh "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP" "tar -C '$remote_src' -xf -"

log "running remote bootstrap (apt + rustup + cargo build + install.sh)"
ssh_vm bash -se <<'REMOTE'
set -euo pipefail

# 1. Base packages. build-essential gives us cc/ld; pkg-config + libssl
# cover the few crates that still link OpenSSL transitively.
if ! command -v cc >/dev/null 2>&1 || ! command -v sqlite3 >/dev/null 2>&1; then
    sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev curl ca-certificates sqlite3 python3 jq rsync
fi

# 2. rustup (only if not already present).
if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
fi
# shellcheck disable=SC1090
source "$HOME/.cargo/env"

cd "$HOME/scryd-src"

# 3. Pin to the toolchain version declared in the repo if present.
if [[ -f rust-toolchain.toml ]]; then
    rustup show >/dev/null
fi

# 4. Release build. -j2 to fit Standard_B2ms without OOM.
echo "=== cargo build --release -p scryd -p scryd-fetch-weights ==="
cargo build --release -j 2 -p scryd -p scryd-fetch-weights

# 5. Stage a release-style bundle and run the real install.sh exactly
#    as a downloaded tarball would.
BUNDLE="$HOME/scryd-bundle"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE"
cp target/release/scryd "$BUNDLE/scryd"
cp target/release/scryd-fetch-weights "$BUNDLE/scryd-fetch-weights"
cp ops/install.sh ops/uninstall.sh ops/scryd.service.in ops/scryd.tmpfiles.in LICENSE "$BUNDLE/"
chmod +x "$BUNDLE/install.sh" "$BUNDLE/uninstall.sh" "$BUNDLE/scryd" "$BUNDLE/scryd-fetch-weights"

echo "=== sudo ./install.sh ==="
sudo "$BUNDLE/install.sh"

echo "=== scryd --version ==="
/usr/local/bin/scryd --version

echo "=== systemctl status scryd (--no-pager) ==="
sudo systemctl status scryd --no-pager -l || true
REMOTE

log "install complete"
