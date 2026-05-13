#!/usr/bin/env bash
# End-to-end test: GreenMail (already running on the host) <- SMTP
# inject -> scryd indexes via IMAP -> scryd search returns hits.
#
# Self-relaunches in a privileged debian:bookworm container with
# --network host so the inner container reaches 127.0.0.1:3143
# (GreenMail's IMAP) and 127.0.0.1:3025 (SMTP).
#
# Locally, before running:
#   docker run -d --rm --name greenmail \
#     -p 3025:3025 -p 3143:3143 \
#     -e GREENMAIL_OPTS="-Dgreenmail.smtp.hostname=0.0.0.0 -Dgreenmail.smtp.port=3025 \
#                        -Dgreenmail.imap.hostname=0.0.0.0 -Dgreenmail.imap.port=3143 \
#                        -Dgreenmail.users=test:test@localhost -Dgreenmail.auth.disabled" \
#     greenmail/standalone:latest
#   cargo build --release -p scryd -p scryd-fetch-weights
#   bash tests/e2e_imap_to_search.sh

set -euo pipefail

if [[ ! -e /.dockerenv && -z "${SCRYD_E2E_INNER:-}" ]]; then
    REPO="$(git rev-parse --show-toplevel)"
    if [[ ! -x "$REPO/target/release/scryd" ]]; then
        echo "FAIL: target/release/scryd missing — run 'cargo build --release -p scryd -p scryd-fetch-weights' first" >&2
        exit 1
    fi
    GMHOST="${SCRYD_TEST_GREENMAIL_HOST:-127.0.0.1}"
    GMPORT="${SCRYD_TEST_GREENMAIL_IMAP_PORT:-3143}"
    if ! (exec 3<>"/dev/tcp/${GMHOST}/${GMPORT}") 2>/dev/null; then
        echo "FAIL: nothing listening on ${GMHOST}:${GMPORT} (expected GreenMail). Start it per the harness comment, then re-run." >&2
        exit 1
    fi
    exec docker run --privileged --rm \
        --network host \
        -e SCRYD_E2E_INNER=1 \
        -v "$REPO:/workspace" \
        -w /workspace \
        debian:bookworm \
        bash tests/e2e_imap_to_search.sh
fi

fail() { echo "FAIL: $*" >&2; exit 1; }
note() { echo "--- $* ---"; }

note "installing prerequisites"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -qq -y --no-install-recommends \
    sudo passwd coreutils sqlite3 ca-certificates curl python3 >/dev/null

note "building fixture bundle"
BUNDLE="$(mktemp -d)"
cp target/release/scryd "$BUNDLE/scryd"
cp target/release/scryd-fetch-weights "$BUNDLE/scryd-fetch-weights"
cp ops/install.sh "$BUNDLE/install.sh"
cp ops/uninstall.sh "$BUNDLE/uninstall.sh"
cp ops/scryd.service.in "$BUNDLE/scryd.service.in"
cp ops/scryd.tmpfiles.in "$BUNDLE/scryd.tmpfiles.in"
cp LICENSE "$BUNDLE/LICENSE"
chmod +x "$BUNDLE/install.sh" "$BUNDLE/uninstall.sh" "$BUNDLE/scryd" "$BUNDLE/scryd-fetch-weights"

note "running install.sh"
(cd "$BUNDLE" && ./install.sh --skip-weights --skip-systemctl)

# Stage the xtr-int4 OpenVINO asset bundle if the outer CI workflow
# pre-fetched it into $REPO/assets-cache/. The host doesn't share its
# /var/lib/scryd with this container, so the outer prime step can't
# touch /var/lib/scryd directly — it leaves the four files under
# $REPO/assets-cache/ and we copy them in here (chowned to scryd).
note "staging xtr-gguf assets into /var/lib/scryd/assets (if pre-fetched)"
if [[ -d /workspace/assets-cache && -e /workspace/assets-cache/xtr.gguf ]]; then
    install -d -o scryd -g scryd -m 0755 /var/lib/scryd/assets
    for f in config.json tokenizer.json xtr.gguf; do
        install -o scryd -g scryd -m 0644 "/workspace/assets-cache/$f" "/var/lib/scryd/assets/$f"
    done
    echo "staged 3 xtr-gguf files into /var/lib/scryd/assets/"
else
    echo "WARN: /workspace/assets-cache/ missing xtr.gguf; daemon will auto-fetch on first start"
fi

note "writing /etc/scryd/config.toml pointing at GreenMail (two accounts)"
cat > /tmp/scryd-config.toml <<'EOF'
[[accounts]]
id = "primary"
host = "127.0.0.1"
port = 3143
user = "test"
password = "test"
tls = false
folders = ["INBOX"]

[[accounts]]
id = "secondary"
host = "127.0.0.1"
port = 3143
user = "test"
password = "test"
tls = false
folders = ["INBOX"]
EOF
install -m 0640 -o scryd -g scryd /tmp/scryd-config.toml /etc/scryd/config.toml

note "injecting 50 messages via SMTP under 'primary' subject prefix"
python3 - <<'PY'
import smtplib
from email.mime.text import MIMEText
s = smtplib.SMTP('127.0.0.1', 3025)
for i in range(50):
    msg = MIMEText(f'body-e2e-{i}')
    msg['From'] = 'alice@localhost'
    msg['To'] = 'test@localhost'
    msg['Subject'] = f'e2e-{i}'
    s.sendmail('alice@localhost', ['test@localhost'], msg.as_string())
s.quit()
print(f'injected 50 messages')
PY

start_daemon() {
    # Production runs the daemon as the scryd system user via the
    # systemd unit. The privileged-docker smoke runs it as root
    # because rusqlite's bundled SQLite misreports the freshly-
    # created meta.sqlite as read-only when the uid switches mid-
    # process via sudo / runuser inside this specific container
    # layout. See task 09 triage for detail.
    XDG_CONFIG_HOME=/etc/scryd \
        XDG_DATA_HOME=/var/lib/scryd \
        XDG_RUNTIME_DIR=/run/scryd \
        /usr/local/bin/scryd serve > /tmp/scryd.log 2>&1 &
    SCRYD_PID=$!
}

note "starting scryd serve in background"
ls -la /var/lib/scryd
sudo -u scryd touch /var/lib/scryd/probe-write && echo "scryd write probe OK" && rm /var/lib/scryd/probe-write
start_daemon

# Cleanup on exit.
trap '
    kill ${SCRYD_PID:-0} 2>/dev/null || true
    wait ${SCRYD_PID:-0} 2>/dev/null || true
    if [[ -n "${KEEP_LOG:-}" ]]; then
        echo "scryd log:"; cat /tmp/scryd.log || true
    fi
' EXIT

note "polling /var/lib/scryd/meta.sqlite for indexed messages"
COUNT=0
DEADLINE=$(($(date +%s) + 60))
while [[ $(date +%s) -lt $DEADLINE ]]; do
    COUNT=$(sqlite3 /var/lib/scryd/meta.sqlite 'SELECT count(*) FROM messages' 2>/dev/null || echo 0)
    # 50 primary + 50 secondary = 100 (each account fetches the same
    # GreenMail INBOX since they're pointed at the same user). We just
    # require >= 50 to confirm at least one account is indexing.
    if [[ "$COUNT" -ge 50 ]]; then
        break
    fi
    sleep 0.5
done

note "sqlite tables and counts"
sqlite3 /var/lib/scryd/meta.sqlite '.tables' 2>&1 || true
sqlite3 /var/lib/scryd/meta.sqlite "SELECT 'messages: ' || count(*) FROM messages; SELECT 'accounts: ' || count(*) FROM accounts; SELECT 'index_queue: ' || count(*) FROM index_queue;" 2>&1 || true

if [[ "$COUNT" -lt 50 ]]; then
    KEEP_LOG=1
    fail "expected >= 50 messages indexed within 60s, got $COUNT"
fi
note "indexed $COUNT messages"

note "running scryd search (open socket; no sudo needed)"
HITS_OUT=$(XDG_RUNTIME_DIR=/run/scryd \
    /usr/local/bin/scryd search "e2e" --limit 5 --json 2>&1 || true)
echo "$HITS_OUT"

if echo "$HITS_OUT" | grep -q '"hits"'; then
    HIT_COUNT=$(echo "$HITS_OUT" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len(d.get("hits",[])))' 2>/dev/null || echo 0)
    if [[ "$HIT_COUNT" -lt 1 ]]; then
        KEEP_LOG=1
        fail "scryd search returned 0 hits; expected >= 1"
    fi
    note "scryd search returned $HIT_COUNT hits"
else
    KEEP_LOG=1
    fail "scryd search did not return JSON hits envelope: $HITS_OUT"
fi

note "filtering by --accounts secondary"
SECONDARY_OUT=$(XDG_RUNTIME_DIR=/run/scryd \
    /usr/local/bin/scryd search "e2e" --accounts secondary --limit 50 --json 2>&1 || true)
echo "$SECONDARY_OUT"

if ! echo "$SECONDARY_OUT" | grep -q '"hits"'; then
    KEEP_LOG=1
    fail "secondary-filtered search returned no JSON envelope: $SECONDARY_OUT"
fi
SECONDARY_BAD=$(echo "$SECONDARY_OUT" | python3 -c '
import sys, json
d = json.load(sys.stdin)
print(sum(1 for h in d.get("hits", []) if h.get("account_id") and h["account_id"] != "secondary"))
' 2>/dev/null || echo unknown)
if [[ "$SECONDARY_BAD" != "0" ]]; then
    KEEP_LOG=1
    fail "secondary-filtered search returned hits with account_id != 'secondary' (count=$SECONDARY_BAD)"
fi
note "secondary-filter assertion passed"

note "killing daemon and restarting to prove witchcraft persistence"
kill -TERM "$SCRYD_PID" 2>/dev/null || true
wait "$SCRYD_PID" 2>/dev/null || true
mv /tmp/scryd.log /tmp/scryd.log.pre-restart || true
start_daemon

# Wait for socket to come back.
DEADLINE=$(($(date +%s) + 30))
while [[ $(date +%s) -lt $DEADLINE ]]; do
    if [[ -S /run/scryd/scryd.sock ]]; then
        break
    fi
    sleep 0.2
done
[[ -S /run/scryd/scryd.sock ]] || { KEEP_LOG=1; fail "socket did not return after restart"; }

note "search after restart (no re-fetch needed)"
POST_OUT=$(XDG_RUNTIME_DIR=/run/scryd \
    /usr/local/bin/scryd search "e2e" --limit 5 --json 2>&1 || true)
echo "$POST_OUT"
if ! echo "$POST_OUT" | grep -q '"hits"'; then
    KEEP_LOG=1
    fail "post-restart search did not return JSON envelope: $POST_OUT"
fi
POST_HITS=$(echo "$POST_OUT" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len(d.get("hits",[])))' 2>/dev/null || echo 0)
if [[ "$POST_HITS" -lt 1 ]]; then
    KEEP_LOG=1
    fail "post-restart search returned 0 hits; witchcraft persistence broken"
fi

echo "OK: e2e indexing + search round-trip + persistence + account_ids filter passed"
