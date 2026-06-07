#!/usr/bin/env bash
# Register the IMAP account (junk@pjly.io by default) on the VM.
# Password is prompted locally (or sourced from $SCRYD_SMOKE_IMAP_PASSWORD)
# and piped over SSH into `scryd add-account --password-stdin` so it
# never lands on local disk and never appears in process args on the VM.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

load_vm

if [[ -z "${SCRYD_SMOKE_IMAP_PASSWORD:-}" ]]; then
    # -s: silent (no echo). -r: don't mangle backslashes.
    printf 'IMAP password for %s @ %s: ' "$SCRYD_SMOKE_IMAP_USER" "$SCRYD_SMOKE_IMAP_HOST" >&2
    read -rs SCRYD_SMOKE_IMAP_PASSWORD
    printf '\n' >&2
fi
[[ -n "$SCRYD_SMOKE_IMAP_PASSWORD" ]] || die "empty password"

log "adding account '$SCRYD_SMOKE_ACCOUNT_ID' ($SCRYD_SMOKE_IMAP_USER @ $SCRYD_SMOKE_IMAP_HOST:$SCRYD_SMOKE_IMAP_PORT)"

# Wait for the daemon to be accepting before issuing add-account
# (install.sh starts the service but witchcraft load takes a moment).
ssh_vm bash -se <<'REMOTE'
set -euo pipefail
deadline=$(( $(date +%s) + 120 ))
until scryd status >/dev/null 2>&1; do
    if (( $(date +%s) >= deadline )); then
        echo "scryd status never succeeded; recent journal:" >&2
        sudo journalctl -u scryd -n 80 --no-pager >&2 || true
        exit 1
    fi
    sleep 2
done
echo "daemon ready"
REMOTE

# Pipe the password over SSH stdin into add-account. The remote shell
# only sees the bytes via stdin; they never appear on argv.
printf '%s' "$SCRYD_SMOKE_IMAP_PASSWORD" | ssh "${SMOKE_SSH_OPTS[@]}" \
    "$SMOKE_VM_USER@$SMOKE_VM_IP" \
    sudo scryd add-account \
        --account-id "$SCRYD_SMOKE_ACCOUNT_ID" \
        --host "$SCRYD_SMOKE_IMAP_HOST" \
        --port "$SCRYD_SMOKE_IMAP_PORT" \
        --user "$SCRYD_SMOKE_IMAP_USER" \
        --password-stdin \
        --folders "$SCRYD_SMOKE_IMAP_FOLDERS"

log "add-account complete; printing status"
ssh_vm scryd status || true
