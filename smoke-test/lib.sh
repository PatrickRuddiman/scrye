# Shared helpers sourced by every script in smoke-test/.
# Conventions:
#   - All state lives under smoke-test/.state/ (gitignored).
#   - `log` prints to stderr so script stdout can still be captured.
#   - `record_finding` appends a JSON object to findings.jsonl; the
#     bug-filer turns each line into one GH issue.

set -euo pipefail

LIB_SOURCE="${BASH_SOURCE[0]:-}"
if [[ -n "$LIB_SOURCE" ]]; then
    SMOKE_ROOT="$(cd "$(dirname "$LIB_SOURCE")" && pwd)"
else
    # Some Windows/MSYS bash -c/source combinations leave BASH_SOURCE empty.
    # In that mode callers are already running from smoke-test/; preserve
    # that behavior instead of silently resolving every path to /.
    SMOKE_ROOT="$(pwd)"
fi
SMOKE_STATE_DIR="$SMOKE_ROOT/.state"
SMOKE_KEY_DIR="$SMOKE_STATE_DIR/keys"
SMOKE_LOG_DIR="$SMOKE_STATE_DIR/logs"
SMOKE_FINDINGS="$SMOKE_STATE_DIR/findings.jsonl"
SMOKE_VM_STATE="$SMOKE_STATE_DIR/vm.json"

mkdir -p "$SMOKE_STATE_DIR" "$SMOKE_KEY_DIR" "$SMOKE_LOG_DIR"

# Load optional config.env (gitignored). Defaults below apply if unset.
if [[ -f "$SMOKE_ROOT/config.env" ]]; then
    # shellcheck disable=SC1091
    source "$SMOKE_ROOT/config.env"
fi

# Secrets live under .state/ (gitignored). Kept separate from config.env
# so the latter stays diff-friendly. Restrictive perms enforced on read.
SMOKE_SECRETS="$SMOKE_STATE_DIR/secrets.env"
if [[ -f "$SMOKE_SECRETS" ]]; then
    chmod 600 "$SMOKE_SECRETS" 2>/dev/null || true
    # shellcheck disable=SC1090
    source "$SMOKE_SECRETS"
fi

: "${SCRYD_SMOKE_RG:=scryd-smoke}"
: "${SCRYD_SMOKE_LOCATION:=eastus2}"
: "${SCRYD_SMOKE_VM_NAME:=scryd-smoke-vm}"
: "${SCRYD_SMOKE_VM_SIZE:=Standard_B4ms}"
: "${SCRYD_SMOKE_VM_IMAGE:=Ubuntu2204}"
: "${SCRYD_SMOKE_VM_USER:=azureuser}"
: "${SCRYD_SMOKE_ACCOUNT_ID:=junk}"
: "${SCRYD_SMOKE_IMAP_USER:=junk@pjly.io}"
: "${SCRYD_SMOKE_IMAP_HOST:=mail.pjly.io}"
: "${SCRYD_SMOKE_IMAP_PORT:=993}"
: "${SCRYD_SMOKE_IMAP_FOLDERS:=INBOX}"
: "${SCRYD_SMOKE_GH_REPO:=PatrickRuddiman/scrye}"
: "${SCRYD_SMOKE_GH_LABEL:=smoke-test}"

# Canonical key location (gitignored under .state/keys/). When this
# tree lives on an NTFS mount (WSL /mnt/c, Cygwin /cygdrive/c), unix
# perms are stuck at 0777 and Linux openssh refuses the key. Mirror
# it once per process into a chmod-respecting filesystem ($HOME/.ssh)
# and point SMOKE_KEY_PATH at that copy.
SMOKE_KEY_SRC="$SMOKE_KEY_DIR/id_ed25519"
case "$SMOKE_KEY_SRC" in
    /mnt/*|/cygdrive/*)
        SMOKE_KEY_PATH="$HOME/.ssh/scryd-smoke-id"
        ;;
    *)
        SMOKE_KEY_PATH="$SMOKE_KEY_SRC"
        ;;
esac

# Copy + lock perms. Called by every script that needs SSH after
# 01_keys.sh has produced SMOKE_KEY_SRC. No-op when the canonical
# path *is* the runtime path.
sync_key() {
    [[ -f "$SMOKE_KEY_SRC" ]] || die "ssh key missing at $SMOKE_KEY_SRC — run 01_keys.sh first"
    if [[ "$SMOKE_KEY_PATH" != "$SMOKE_KEY_SRC" ]]; then
        mkdir -p "$(dirname "$SMOKE_KEY_PATH")"
        cp -f "$SMOKE_KEY_SRC" "$SMOKE_KEY_PATH"
        cp -f "${SMOKE_KEY_SRC}.pub" "${SMOKE_KEY_PATH}.pub"
    fi
    chmod 600 "$SMOKE_KEY_PATH" 2>/dev/null || true
    chmod 644 "${SMOKE_KEY_PATH}.pub" 2>/dev/null || true
}

log()  { printf '\033[1;36m[%s]\033[0m %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
warn() { printf '\033[1;33m[%s] WARN\033[0m %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die()  { printf '\033[1;31m[%s] FAIL\033[0m %s\n' "$(date +%H:%M:%S)" "$*" >&2; exit 1; }

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

# Resolve `gh` even when WSL bash hides the user's scoop shims. Sets
# global SMOKE_GH to the first working binary. Use $SMOKE_GH instead
# of bare `gh` in scripts that may run under inconsistent shells.
resolve_gh() {
    if command -v gh >/dev/null 2>&1; then
        SMOKE_GH="gh"
        return 0
    fi
    local cand
    for cand in \
        "/mnt/c/Users/$(whoami)/scoop/shims/gh.exe" \
        "/mnt/c/Users/${USER:-_}/scoop/shims/gh.exe" \
        "/mnt/c/Users/${USERNAME:-_}/scoop/shims/gh.exe" \
        "/mnt/c/ProgramData/scoop/shims/gh.exe" \
        "/mnt/c/Program Files/GitHub CLI/gh.exe" \
        "/c/Users/$(whoami)/scoop/shims/gh.exe" \
        "/c/ProgramData/scoop/shims/gh.exe"; do
        if [[ -x "$cand" ]]; then
            SMOKE_GH="$cand"
            return 0
        fi
    done
    # Windows usernames rarely match WSL's whoami (prudd vs pruddiman).
    # Glob every Users/*/scoop install so we don't depend on a match.
    local hit
    for hit in /mnt/c/Users/*/scoop/shims/gh.exe /c/Users/*/scoop/shims/gh.exe; do
        if [[ -x "$hit" ]]; then
            SMOKE_GH="$hit"
            return 0
        fi
    done
    return 1
}

# Read the VM record written by 02_vm.sh. Sets SMOKE_VM_IP and
# SMOKE_VM_USER. Errors if VM hasn't been provisioned.
load_vm() {
    [[ -f "$SMOKE_VM_STATE" ]] || die "no VM state at $SMOKE_VM_STATE — run 02_vm.sh first"
    need_cmd python3
    local vm_record
    vm_record="$(python3 - "$SMOKE_VM_STATE" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as f:
    data = json.load(f)
print(data.get("ip", ""))
print(data.get("user", ""))
PY
)"
    SMOKE_VM_IP="${vm_record%%$'\n'*}"
    SMOKE_VM_USER="${vm_record#*$'\n'}"
    [[ -n "$SMOKE_VM_IP" ]] || die "VM state missing 'ip'"
    [[ -n "$SMOKE_VM_USER" ]] || die "VM state missing 'user'"
}

# SSH/SCP wrappers that always use our key and skip host-key prompts
# (the VM is ephemeral; trust-on-first-use is fine here).
SMOKE_SSH_OPTS=(
    -i "$SMOKE_KEY_PATH"
    -o StrictHostKeyChecking=no
    -o UserKnownHostsFile=/dev/null
    -o LogLevel=ERROR
    -o ConnectTimeout=15
    -o ServerAliveInterval=30
)

ssh_vm() {
    load_vm
    sync_key
    ssh "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP" "$@"
}

scp_to_vm() {
    # scp_to_vm <local_src...> <remote_dest>
    load_vm
    # The conventional form is "scp ... src... user@host:dest".
    local args=("$@")
    local n=${#args[@]}
    local dest="${args[$((n-1))]}"
    local srcs=("${args[@]:0:$((n-1))}")
    scp "${SMOKE_SSH_OPTS[@]}" -r "${srcs[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP:$dest"
}

# record_finding <check_id> <title> <summary> [log_path]
# Appends one JSON object per line to findings.jsonl. The bug-filer
# dedupes on `title`, so keep titles stable across runs.
record_finding() {
    local check_id="$1" title="$2" summary="$3" log_path="${4:-}"
    need_cmd python3
    python3 - "$check_id" "$title" "$summary" "$log_path" "$SMOKE_FINDINGS" <<'PY'
import json, sys, os, datetime
check_id, title, summary, log_path, out_path = sys.argv[1:6]
rec = {
    "ts": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    "check": check_id,
    "title": title,
    "summary": summary,
    "log": log_path or None,
}
with open(out_path, "a", encoding="utf-8") as f:
    f.write(json.dumps(rec) + "\n")
PY
    warn "finding recorded: [$check_id] $title"
}
