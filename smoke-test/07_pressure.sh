#!/usr/bin/env bash
# Pressure-test helper for the smoke VM. Subcommands are intentionally
# resumable because cold indexing/reindexing can run longer than an SSH
# session or local tool timeout.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

usage() {
    cat <<'EOF'
usage: ./07_pressure.sh <command>

commands:
  push              copy pressure scripts to the smoke VM
  load              run bounded API/CLI/load coverage against current VM state
  monitor-cold      start/continue low-overhead cold-index queue monitor
  reindex           start live reindex monitor (triggers `scryd reindex` once)
  status            show pressure units + current message/index queue state
  collect DIR       pull /tmp/scryd-pressure artifacts into DIR

Typical sequence after ./run.sh:
  ./07_pressure.sh push
  ./07_pressure.sh load
  ./07_pressure.sh status
  ./07_pressure.sh collect .state/pressure/latest

Cold-backlog measurement sequence:
  ./07_pressure.sh push
  sudo/manual: reset /var/lib/scryd metadata/index/raw state if desired
  ./07_pressure.sh monitor-cold
  ./07_pressure.sh status
  ./07_pressure.sh collect .state/pressure/cold

Reindex sequence:
  ./07_pressure.sh push
  ./07_pressure.sh reindex
  ./07_pressure.sh status
  ./07_pressure.sh collect .state/pressure/reindex
EOF
}

push_scripts() {
    load_vm
    sync_key
    scp_to_vm \
        "$SCRIPT_DIR/pressure_runner.py" \
        "$SCRIPT_DIR/cold_monitor.py" \
        "$SCRIPT_DIR/reindex_monitor.py" \
        /tmp/
    ssh_vm "chmod 755 /tmp/pressure_runner.py /tmp/cold_monitor.py /tmp/reindex_monitor.py && python3 -m py_compile /tmp/pressure_runner.py /tmp/cold_monitor.py /tmp/reindex_monitor.py"
}

start_load() {
    push_scripts
    ssh_vm "sudo systemctl reset-failed scryd-pressure-load.service 2>/dev/null || true; sudo systemd-run --unit scryd-pressure-load --property=WorkingDirectory=/tmp /usr/bin/python3 /tmp/pressure_runner.py --read-seconds 90 --mixed-seconds 180 --semantic-seconds 120 --skip-reindex; systemctl is-active scryd-pressure-load.service"
}

start_cold_monitor() {
    push_scripts
    ssh_vm "sudo systemctl reset-failed scryd-pressure-monitor.service 2>/dev/null || true; sudo systemd-run --unit scryd-pressure-monitor --property=WorkingDirectory=/tmp --setenv=SCRYD_MONITOR_MAX_SECONDS=432000 --setenv=SCRYD_MONITOR_STABLE_SECONDS=120 /usr/bin/python3 /tmp/cold_monitor.py; systemctl is-active scryd-pressure-monitor.service"
}

start_reindex() {
    push_scripts
    ssh_vm "sudo systemctl reset-failed scryd-pressure-reindex.service 2>/dev/null || true; sudo systemd-run --unit scryd-pressure-reindex --property=WorkingDirectory=/tmp --setenv=SCRYD_REINDEX_MAX_SECONDS=432000 --setenv=SCRYD_REINDEX_STABLE_SECONDS=120 /usr/bin/python3 /tmp/reindex_monitor.py; systemctl is-active scryd-pressure-reindex.service"
}

status_pressure() {
    load_vm
    sync_key
    ssh_vm "echo units; for u in scryd-pressure-load scryd-pressure-monitor scryd-pressure-reindex scryd-pressure-reindex-continue; do printf '%s ' \"\$u\"; systemctl is-active \"\$u.service\" 2>/dev/null || true; done; echo db; sudo python3 -c 'import sqlite3,json,pathlib; db=pathlib.Path(\"/var/lib/scryd/meta.sqlite\"); con=sqlite3.connect(f\"file:{db}?mode=ro\", uri=True); c=con.cursor(); out={t:c.execute(f\"select count(*) from {t}\").fetchone()[0] for t in [\"messages\",\"index_queue\",\"accounts\",\"sync_state\"]}; out[\"queue_ready\"]=c.execute(\"select count(*) from index_queue where failed_permanent=0\").fetchone()[0]; out[\"queue_failed\"]=c.execute(\"select count(*) from index_queue where failed_permanent!=0\").fetchone()[0]; print(json.dumps(out, indent=2))'; echo status; scryd status || true"
}

collect_pressure() {
    local dest="${1:?collect requires destination directory}"
    bash "$SCRIPT_DIR/pull_pressure_artifacts.sh" "$dest"
}

cmd="${1:-}"
case "$cmd" in
    push) push_scripts ;;
    load) start_load ;;
    monitor-cold) start_cold_monitor ;;
    reindex) start_reindex ;;
    status) status_pressure ;;
    collect) shift; collect_pressure "${1:-}" ;;
    -h|--help|help|"") usage ;;
    *) usage >&2; exit 2 ;;
esac
