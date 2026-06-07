#!/usr/bin/env bash
# Mint an RSA 4096 keypair dedicated to the smoke VM. RSA (not ed25519)
# because az CLI 2.82's `vm create --ssh-key-values` rejects ed25519.
# Idempotent: existing key at the expected path is reused.
# The key is *not* passphrase-protected because every script in this
# folder runs unattended; it lives under .state/ which is gitignored.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

need_cmd ssh-keygen

if [[ -f "$SMOKE_KEY_SRC" && -f "${SMOKE_KEY_SRC}.pub" ]]; then
    log "ssh key already present at $SMOKE_KEY_SRC"
else
    log "minting rsa-4096 key at $SMOKE_KEY_SRC"
    ssh-keygen -t rsa -b 4096 -N "" -C "scryd-smoke@$(hostname)" -f "$SMOKE_KEY_SRC" >/dev/null
fi

# Mirror to a chmod-respecting location (no-op on Linux/Mac native FS).
sync_key

log "public key:"
cat "${SMOKE_KEY_SRC}.pub" >&2
