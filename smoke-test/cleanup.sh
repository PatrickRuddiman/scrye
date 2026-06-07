#!/usr/bin/env bash
# Delete the smoke resource group. Fire-and-forget by default; pass
# --wait to block until Azure confirms deletion.
#
# Does NOT delete the local SSH key or findings/logs — those are useful
# evidence after the VM is gone. Run `rm -rf smoke-test/.state` to nuke
# state.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

need_cmd az

wait_flag="--no-wait"
if [[ "${1:-}" == "--wait" ]]; then
    wait_flag=""
fi

if ! az group show -n "$SCRYD_SMOKE_RG" >/dev/null 2>&1; then
    log "resource group '$SCRYD_SMOKE_RG' does not exist; nothing to do"
    rm -f "$SMOKE_VM_STATE"
    exit 0
fi

log "deleting resource group '$SCRYD_SMOKE_RG' ($wait_flag)"
az group delete -n "$SCRYD_SMOKE_RG" --yes $wait_flag

rm -f "$SMOKE_VM_STATE"
log "done"
