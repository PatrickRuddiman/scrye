#!/usr/bin/env bash
# Provision (or re-use) an Azure VM for the smoke run. Writes the
# resulting public IP + admin username to .state/vm.json so the
# rest of the pipeline can SSH in without re-querying Azure.
#
# Idempotent: re-running on an existing RG just refreshes vm.json.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

need_cmd az
need_cmd python3
[[ -f "${SMOKE_KEY_SRC}.pub" ]] || die "ssh key missing — run 01_keys.sh first"

# Make sure we have an active az session before touching anything.
if ! az account show >/dev/null 2>&1; then
    die "az not logged in — run 'az login' first"
fi

sub_name="$(az account show --query name -o tsv | tr -d '\r\n')"
sub_id="$(az account show --query id -o tsv | tr -d '\r\n')"
log "subscription: $sub_name ($sub_id)"

if az group show -n "$SCRYD_SMOKE_RG" >/dev/null 2>&1; then
    log "resource group '$SCRYD_SMOKE_RG' already exists"
else
    log "creating resource group '$SCRYD_SMOKE_RG' in '$SCRYD_SMOKE_LOCATION'"
    az group create -n "$SCRYD_SMOKE_RG" -l "$SCRYD_SMOKE_LOCATION" >/dev/null
fi

if az vm show -g "$SCRYD_SMOKE_RG" -n "$SCRYD_SMOKE_VM_NAME" >/dev/null 2>&1; then
    log "vm '$SCRYD_SMOKE_VM_NAME' already exists; reusing"
else
    log "creating vm '$SCRYD_SMOKE_VM_NAME' (size=$SCRYD_SMOKE_VM_SIZE image=$SCRYD_SMOKE_VM_IMAGE)"
    az vm create \
        -g "$SCRYD_SMOKE_RG" \
        -n "$SCRYD_SMOKE_VM_NAME" \
        --image "$SCRYD_SMOKE_VM_IMAGE" \
        --size "$SCRYD_SMOKE_VM_SIZE" \
        --admin-username "$SCRYD_SMOKE_VM_USER" \
        --ssh-key-values "$(cat "${SMOKE_KEY_SRC}.pub")" \
        --public-ip-sku Standard \
        --nsg-rule SSH \
        --output none

    # Lock the NSG to the workstation's egress IP — Azure's default
    # SSH rule is open to the internet which is fine for a 10-minute
    # smoke run but worth tightening when possible. Best-effort.
    egress_ip="$(curl -fsSL --max-time 5 https://api.ipify.org 2>/dev/null || true)"
    if [[ -n "$egress_ip" ]]; then
        log "narrowing NSG SSH rule to $egress_ip/32"
        nsg_name="$(az network nsg list -g "$SCRYD_SMOKE_RG" --query '[0].name' -o tsv | tr -d '\r\n')"
        az network nsg rule update -g "$SCRYD_SMOKE_RG" --nsg-name "$nsg_name" \
            -n default-allow-ssh --source-address-prefixes "$egress_ip/32" \
            --output none 2>/dev/null || warn "could not narrow NSG rule (continuing)"
    fi
fi

ip="$(az vm show -d -g "$SCRYD_SMOKE_RG" -n "$SCRYD_SMOKE_VM_NAME" --query publicIps -o tsv | tr -d '\r\n')"
[[ -n "$ip" ]] || die "vm has no public IP yet"

python3 - "$ip" "$SCRYD_SMOKE_VM_USER" "$SMOKE_VM_STATE" <<'PY'
import json, sys
ip, user, out = sys.argv[1:]
with open(out, "w", encoding="utf-8") as f:
    json.dump({"ip": ip, "user": user}, f, indent=2)
PY
log "vm public ip: $ip"

# Wait for SSH to actually accept.
log "waiting for sshd to accept connections..."
deadline=$(( $(date +%s) + 180 ))
while (( $(date +%s) < deadline )); do
    if ssh "${SMOKE_SSH_OPTS[@]}" "$SCRYD_SMOKE_VM_USER@$ip" true 2>/dev/null; then
        log "ssh OK"
        exit 0
    fi
    sleep 3
done
die "ssh did not come up within 180s"
