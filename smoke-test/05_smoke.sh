#!/usr/bin/env bash
# Drive scryd through a series of operator-realistic checks over SSH.
# Each check writes its raw output to .state/logs/<check>.log; failures
# get appended to .state/findings.jsonl which `06_bugs.sh` turns into
# GH issues.
#
# Add new checks as you discover regressions worth pinning. Keep each
# check deterministic-titled — the bug-filer dedupes on `title`.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

load_vm
sync_key

PASS=0
FAIL=0

# run_check <id> <title-on-failure> <bash-command-string>
# - Captures combined stdout/stderr to logs/<id>.log
# - Records a finding on non-zero exit
run_check() {
    local id="$1" title="$2" cmd="$3"
    local logf="$SMOKE_LOG_DIR/${id}.log"
    log "check: $id"
    if ssh "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP" "$cmd" >"$logf" 2>&1; then
        PASS=$((PASS+1))
        return 0
    fi
    FAIL=$((FAIL+1))
    local tail
    tail="$(tail -n 40 "$logf" 2>/dev/null | sed 's/^/    /')"
    record_finding "$id" "$title" "Smoke check '$id' failed against $SMOKE_VM_IP. Last log lines:\n$tail" "$logf"
}

# run_assert <id> <title> <bash-cmd> <assertion-bash> — runs cmd, then
# evaluates assertion against the captured log via $LOG variable.
run_assert() {
    local id="$1" title="$2" cmd="$3" assertion="$4"
    local logf="$SMOKE_LOG_DIR/${id}.log"
    log "check: $id"
    ssh "${SMOKE_SSH_OPTS[@]}" "$SMOKE_VM_USER@$SMOKE_VM_IP" "$cmd" >"$logf" 2>&1 || true
    local rc=0
    LOG="$logf" bash -c "$assertion" || rc=$?
    if (( rc == 0 )); then
        PASS=$((PASS+1))
        return 0
    fi
    FAIL=$((FAIL+1))
    local tail
    tail="$(tail -n 40 "$logf" 2>/dev/null | sed 's/^/    /')"
    record_finding "$id" "$title" "Smoke assertion '$id' failed against $SMOKE_VM_IP. Last log lines:\n$tail" "$logf"
}

# Truncate findings + logs from any prior run so each invocation is a
# clean slate. Re-running the bug-filer is still idempotent because it
# dedupes against existing open issues, not against this file.
: >"$SMOKE_FINDINGS"
rm -f "$SMOKE_LOG_DIR"/*.log

# --- Baseline: binary and service health ---
run_check version          "smoke: 'scryd --version' failed"            "scryd --version"
run_check status_daemon    "smoke: 'scryd status' failed"               "scryd status"
run_check systemd_active   "smoke: scryd.service is not 'active'"       "systemctl is-active scryd"

# --- Account visibility ---
# After 04_account.sh, the account must appear in `scryd status`. Use
# python so a missing field is a 1, not a grep miss masquerading as 0.
run_assert account_visible \
    "smoke: account '$SCRYD_SMOKE_ACCOUNT_ID' missing from 'scryd status'" \
    "scryd status" \
    'python3 -c "import json,sys,os; s=json.load(open(os.environ[\"LOG\"])); ids=[a[\"account_id\"] for a in s.get(\"accounts\",[])]; sys.exit(0 if \"'"$SCRYD_SMOKE_ACCOUNT_ID"'\" in ids else 1)"'

# --- Health probe: the storage health enum reports successful sync as
# `Active` (see AccountHealth::Active), not `Healthy`.
run_assert account_health \
    "smoke: account '$SCRYD_SMOKE_ACCOUNT_ID' is not active" \
    "scryd status" \
    'python3 -c "import json,sys,os; s=json.load(open(os.environ[\"LOG\"])); a=next((x for x in s.get(\"accounts\",[]) if x[\"account_id\"]==\"'"$SCRYD_SMOKE_ACCOUNT_ID"'\"),None); sys.exit(0 if a and a.get(\"health\")==\"Active\" else 1)"'

# --- First sync: wait up to 3 min for the supervisor to fetch at
# least one UID. Skipped only when the account is not active.
if [[ -s "$SMOKE_LOG_DIR/account_health.log" ]] && \
   python3 -c 'import json,sys; s=json.load(open(sys.argv[1])); a=next((x for x in s.get("accounts",[]) if x.get("account_id")==sys.argv[2]),None); sys.exit(0 if a and a.get("health")=="Active" else 1)' "$SMOKE_LOG_DIR/account_health.log" "$SCRYD_SMOKE_ACCOUNT_ID"; then
    run_assert first_sync \
        "smoke: account '$SCRYD_SMOKE_ACCOUNT_ID' did not begin indexing within 3m" \
        "deadline=\$((\$(date +%s)+180)); while (( \$(date +%s) < deadline )); do status=\$(scryd status 2>&1) && { printf '%s\n' \"\$status\"; printf '%s\n' \"\$status\" | python3 -c 'import json,sys; s=json.load(sys.stdin); a=next((x for x in s[\"accounts\"] if x[\"account_id\"]==\"$SCRYD_SMOKE_ACCOUNT_ID\"),None); sys.exit(0 if a and a.get(\"last_seen_uid\") else 1)' && exit 0; }; sleep 5; done; scryd status; exit 1" \
        'python3 -c "import json,os,sys; lines=[l for l in open(os.environ[\"LOG\"], encoding=\"utf-8\").read().splitlines() if l.strip().startswith(\"{\")]; s=json.loads(lines[-1]); a=next((x for x in s.get(\"accounts\",[]) if x.get(\"account_id\")==\"'"$SCRYD_SMOKE_ACCOUNT_ID"'\"),None); sys.exit(0 if a and a.get(\"last_seen_uid\") else 1)"'
else
    log "skipping first_sync — account not Active"
fi

# --- Search smoke ---
# We don't know what's in the inbox, so we issue a generic single-letter
# query and assert that we get back a JSON envelope (not a 5xx or a
# parser error). Zero hits is still a pass — the daemon answering at
# all is what we're testing.
run_assert search_smoke \
    "smoke: 'scryd search' did not return a JSON hits envelope" \
    "deadline=\$((\$(date +%s)+180)); while (( \$(date +%s) < deadline )); do out=\$(scryd search 'e' --limit 5 --json 2>&1) && printf '%s\n' \"\$out\" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)' && { printf '%s\n' \"\$out\"; exit 0; }; printf '%s\n' \"\$out\" >&2; sleep 5; done; exit 1" \
    'python3 -c "import json,os,sys; lines=open(os.environ[\"LOG\"], encoding=\"utf-8\").read().splitlines(); d=json.loads(lines[-1]); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)"'

# --- account-scoped search ---
run_assert search_scoped \
    "smoke: scoped 'scryd search --accounts $SCRYD_SMOKE_ACCOUNT_ID' did not return a JSON envelope" \
    "deadline=\$((\$(date +%s)+180)); while (( \$(date +%s) < deadline )); do out=\$(scryd search 'e' --accounts '$SCRYD_SMOKE_ACCOUNT_ID' --limit 5 --json 2>&1) && printf '%s\n' \"\$out\" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)' && { printf '%s\n' \"\$out\"; exit 0; }; printf '%s\n' \"\$out\" >&2; sleep 5; done; exit 1" \
    'python3 -c "import json,os,sys; lines=open(os.environ[\"LOG\"], encoding=\"utf-8\").read().splitlines(); d=json.loads(lines[-1]); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)"'

# --- semantic mode smoke ---
run_assert search_semantic \
    "smoke: semantic-mode 'scryd search --mode semantic' did not return a JSON envelope" \
    "deadline=\$((\$(date +%s)+180)); while (( \$(date +%s) < deadline )); do out=\$(scryd search 'invoice' --mode semantic --limit 5 --json 2>&1) && printf '%s\n' \"\$out\" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)' && { printf '%s\n' \"\$out\"; exit 0; }; printf '%s\n' \"\$out\" >&2; sleep 5; done; exit 1" \
    'python3 -c "import json,os,sys; lines=open(os.environ[\"LOG\"], encoding=\"utf-8\").read().splitlines(); d=json.loads(lines[-1]); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)"'

# --- Restart persistence ---
# Stop + start the daemon and confirm searches still work without a
# refetch (witchcraft index must persist).
run_assert restart_persistence \
    "smoke: scryd did not recover after systemctl restart" \
    "sudo systemctl restart scryd; deadline=\$((\$(date +%s)+180)); while (( \$(date +%s) < deadline )); do scryd status >/dev/null 2>&1 && break; sleep 2; done; deadline=\$((\$(date +%s)+180)); while (( \$(date +%s) < deadline )); do out=\$(scryd search 'e' --limit 1 --json 2>&1) && printf '%s\n' \"\$out\" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)' && { printf '%s\n' \"\$out\"; exit 0; }; printf '%s\n' \"\$out\" >&2; sleep 5; done; exit 1" \
    'python3 -c "import json,os,sys; lines=open(os.environ[\"LOG\"], encoding=\"utf-8\").read().splitlines(); d=json.loads(lines[-1]); sys.exit(0 if isinstance(d.get(\"hits\"), list) else 1)"'

# --- add-account reconcile contract ---
# After `scryd add-account`, /status MUST surface the new account
# without a daemon restart (the CLI prints "changes picked up by
# daemon (no restart needed)" so the operator trusts the live state).
# Drives a remove → add → poll cycle and fails if /status is empty
# after 30s. Run last because it perturbs the configured account.
run_assert addaccount_reconcile \
    "smoke: 'scryd add-account' does not reconcile live — /status empty until restart" \
    "set -e
# Reset to the empty-config baseline so reconcile is exercised against
# a daemon that has zero accounts loaded — that's the regression case.
sudo scryd remove-account '$SCRYD_SMOKE_ACCOUNT_ID' --yes 2>/dev/null || true
sudo systemctl restart scryd
for i in \$(seq 1 60); do scryd status >/dev/null 2>&1 && break; sleep 1; done
echo PRE_ADD:\$(scryd status)
printf '%s' '$SCRYD_SMOKE_IMAP_PASSWORD' | sudo scryd add-account \
    --account-id '$SCRYD_SMOKE_ACCOUNT_ID' \
    --host '$SCRYD_SMOKE_IMAP_HOST' \
    --port '$SCRYD_SMOKE_IMAP_PORT' \
    --user '$SCRYD_SMOKE_IMAP_USER' \
    --password-stdin \
    --folders '$SCRYD_SMOKE_IMAP_FOLDERS'
deadline=\$((\$(date +%s)+30))
while (( \$(date +%s) < deadline )); do
    if scryd status | python3 -c 'import json,sys; s=json.load(sys.stdin); ids=[a[\"account_id\"] for a in s.get(\"accounts\",[])]; sys.exit(0 if \"$SCRYD_SMOKE_ACCOUNT_ID\" in ids else 1)' 2>/dev/null; then
        echo POST_ADD_LIVE
        exit 0
    fi
    sleep 2
done
echo TIMEOUT
echo POST_ADD:\$(scryd status)
exit 1" \
    'grep -q POST_ADD_LIVE "$LOG"'

# --- tls field written by add-account ---
# README documents `tls = true` as part of the minimal [[accounts]]
# table. add-account should write it (or scryd should document the
# default explicitly). This check inspects the config file written
# by the prior step and fails if tls is missing.
run_assert addaccount_tls_field \
    "smoke: 'scryd add-account' omits 'tls' field from config.toml" \
    "sudo grep -A6 \"id = \\\"$SCRYD_SMOKE_ACCOUNT_ID\\\"\" /etc/scryd/config.toml" \
    'grep -q "^tls = " "$LOG"'

log "---- summary ----"
log "passed: $PASS  failed: $FAIL"
log "findings: $(wc -l < "$SMOKE_FINDINGS" | tr -d ' ')"
log "logs:     $SMOKE_LOG_DIR"

if (( FAIL > 0 )); then
    exit 1
fi
