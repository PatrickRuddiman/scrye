#!/usr/bin/env bash
# End-to-end test: GreenMail (already running on the host) <- SMTP
# inject -> scryd fetches via IMAP -> scryd indexes with witchcraft ->
# search over the MCP server returns hits.
#
# scryd's only surface is the MCP server on loopback TCP, so this drives
# search the way a real MCP client would (tests/mcp_client.py speaks the
# Streamable-HTTP transport with stdlib only).
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

MCP_URL="http://127.0.0.1:7878/mcp"

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
cp LICENSE "$BUNDLE/LICENSE"
chmod +x "$BUNDLE/install.sh" "$BUNDLE/uninstall.sh" "$BUNDLE/scryd" "$BUNDLE/scryd-fetch-weights"

note "running install.sh"
(cd "$BUNDLE" && ./install.sh --skip-weights --skip-systemctl)

# Stage the xtr-int4 asset bundle if the outer CI workflow pre-fetched it into
# $REPO/assets-cache/. The host doesn't share its /var/lib/scryd with this
# container, so the outer prime step leaves the files under $REPO/assets-cache/
# and we copy them in here (chowned to scryd). Otherwise the daemon auto-fetches.
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

# Both accounts log in as GreenMail user 'test', so both match USER_EMAIL=test
# and the daemon fetches + indexes both (each mirrors the same INBOX).
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

note "injecting 50 messages via SMTP"
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
print('injected 50 messages')
PY

start_daemon() {
    # USER_EMAIL is mandatory and scopes everything to the matching account(s).
    # The daemon binds its MCP server to loopback TCP (default 127.0.0.1:7878);
    # there is no Unix socket and no `serve` subcommand. We run it as root here
    # because rusqlite's bundled SQLite misreports the freshly-created
    # meta.sqlite as read-only when the uid switches mid-process via sudo inside
    # this specific container layout (see task 09 triage).
    USER_EMAIL=test \
        SCRYD_MCP_BIND=127.0.0.1:7878 \
        XDG_CONFIG_HOME=/etc/scryd \
        XDG_DATA_HOME=/var/lib/scryd \
        /usr/local/bin/scryd > /tmp/scryd.log 2>&1 &
    SCRYD_PID=$!
}

# Poll the MCP server until it advertises the `search` tool (the daemon binds
# MCP only after witchcraft finishes loading, so the port appears late).
wait_for_mcp() {
    local deadline=$(($(date +%s) + ${1:-120}))
    while [[ $(date +%s) -lt $deadline ]]; do
        if python3 tests/mcp_client.py "$MCP_URL" tools 2>/dev/null | grep -q '"search"'; then
            return 0
        fi
        sleep 1
    done
    return 1
}

note "starting scryd in background"
start_daemon

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
    # 50 primary + 50 secondary = 100 (both fetch the same GreenMail INBOX). We
    # only require >= 50 to confirm at least one account is indexing.
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

# The drainer flushes after each batch; wait for the queue to drain before
# searching so witchcraft has caught up and search can return hits.
note "polling index_queue for drain completion"
QUEUE=$(sqlite3 /var/lib/scryd/meta.sqlite 'SELECT count(*) FROM index_queue WHERE failed_permanent = 0' 2>/dev/null || echo 0)
DEADLINE=$(($(date +%s) + 120))
while [[ "$QUEUE" -gt 0 && $(date +%s) -lt $DEADLINE ]]; do
    sleep 1
    QUEUE=$(sqlite3 /var/lib/scryd/meta.sqlite 'SELECT count(*) FROM index_queue WHERE failed_permanent = 0' 2>/dev/null || echo 0)
done
note "index_queue depth after drain wait: $QUEUE"

note "waiting for the MCP server to advertise its tools"
wait_for_mcp 120 || { KEEP_LOG=1; fail "MCP server did not advertise the search tool within 120s"; }

note "search 'e2e' over MCP"
HITS_OUT=$(python3 tests/mcp_client.py "$MCP_URL" search "e2e" --limit 5 2>&1 || true)
echo "$HITS_OUT"
HIT_COUNT=$(echo "$HITS_OUT" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len(d.get("hits",[])))' 2>/dev/null || echo 0)
if [[ "$HIT_COUNT" -lt 1 ]]; then
    KEEP_LOG=1
    fail "MCP search returned 0 hits; expected >= 1 (out: $HITS_OUT)"
fi
note "MCP search returned $HIT_COUNT hits"

note "narrowing search to account_ids=[secondary] over MCP"
SECONDARY_OUT=$(python3 tests/mcp_client.py "$MCP_URL" search "e2e" --accounts secondary --limit 50 2>&1 || true)
echo "$SECONDARY_OUT"
SECONDARY_BAD=$(echo "$SECONDARY_OUT" | python3 -c '
import sys, json
d = json.load(sys.stdin)
print(sum(1 for h in d.get("hits", []) if h.get("account_id") and h["account_id"] != "secondary"))
' 2>/dev/null || echo unknown)
if [[ "$SECONDARY_BAD" != "0" ]]; then
    KEEP_LOG=1
    fail "secondary-narrowed search returned hits with account_id != 'secondary' (count=$SECONDARY_BAD)"
fi
note "account narrowing assertion passed"

note "killing daemon and restarting to prove witchcraft persistence"
kill -TERM "$SCRYD_PID" 2>/dev/null || true
wait "$SCRYD_PID" 2>/dev/null || true
mv /tmp/scryd.log /tmp/scryd.log.pre-restart || true
start_daemon

note "waiting for MCP server after restart"
wait_for_mcp 120 || { KEEP_LOG=1; fail "MCP server did not come back within 120s after restart"; }

note "search after restart (no re-fetch needed)"
POST_OUT=$(python3 tests/mcp_client.py "$MCP_URL" search "e2e" --limit 5 2>&1 || true)
echo "$POST_OUT"
POST_HITS=$(echo "$POST_OUT" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len(d.get("hits",[])))' 2>/dev/null || echo 0)
if [[ "$POST_HITS" -lt 1 ]]; then
    KEEP_LOG=1
    fail "post-restart search returned 0 hits; witchcraft persistence broken"
fi

echo "OK: e2e fetch + index + MCP search round-trip + persistence + account narrowing passed"
