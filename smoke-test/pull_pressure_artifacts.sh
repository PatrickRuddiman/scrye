#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"
DEST="${1:?destination directory required}"
load_vm
sync_key
python3 - "$DEST" <<'PY'
import os, sys
os.makedirs(sys.argv[1], exist_ok=True)
PY
scp "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP:/tmp/scryd-pressure/REPORT.md" "$DEST/REPORT.md" 2>/dev/null || true
scp "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP:/tmp/scryd-pressure/raw/*.json" "$DEST/"
scp "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP:/tmp/scryd-pressure/raw/*.jsonl" "$DEST/" 2>/dev/null || true
