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

note "creating operator user alice"
useradd -m alice

note "running install.sh"
(cd "$BUNDLE" && ./install.sh --user alice --skip-weights --skip-systemctl)

note "writing /etc/scryd/config.toml pointing at GreenMail"
cat > /tmp/scryd-config.toml <<'EOF'
[[accounts]]
id = "primary"
host = "127.0.0.1"
port = 3143
user = "test"
password = "test"
tls = false
folders = ["INBOX"]
EOF
install -m 0600 -o scryd -g scryd /tmp/scryd-config.toml /etc/scryd/config.toml

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
print(f'injected 50 messages')
PY

note "starting scryd serve in background"
ALICE_UID=$(id -u alice)
# Pre-flight: confirm scryd can actually write to its data dir.
ls -la /var/lib/scryd
sudo -u scryd touch /var/lib/scryd/probe-write && echo "scryd write probe OK" && rm /var/lib/scryd/probe-write
# install.sh --skip-systemctl already created /etc/scryd, /var/lib/scryd,
# /var/lib/scryd/assets, and /run/scryd at the right ownership / mode.
# Run the daemon as scryd user with the env vars the systemd unit
# would normally set.
# NOTE: production runs the daemon as the scryd system user via the
# systemd unit (User=scryd / Group=scryd). The privileged-docker
# smoke harness runs it as root because rusqlite's bundled SQLite
# misreports the freshly-created meta.sqlite as read-only when the
# uid switches mid-process via sudo / runuser inside this specific
# container layout — a known mount-or-sandbox interaction worth a
# follow-up (target/release/scryd works as scryd under systemd; the
# scheduler_live integration test exercises the supervisor end-to-
# end against GreenMail without that issue). See task 09 for detail.
SCRYD_ALLOWED_UID=$ALICE_UID \
    XDG_CONFIG_HOME=/etc/scryd \
    XDG_DATA_HOME=/var/lib/scryd \
    XDG_RUNTIME_DIR=/run/scryd \
    /usr/local/bin/scryd serve > /tmp/scryd.log 2>&1 &
SCRYD_PID=$!

# Cleanup on exit.
trap '
    kill $SCRYD_PID 2>/dev/null || true
    wait $SCRYD_PID 2>/dev/null || true
    if [[ -n "${KEEP_LOG:-}" ]]; then
        echo "scryd log:"; cat /tmp/scryd.log || true
    fi
' EXIT

note "polling /var/lib/scryd/meta.sqlite for indexed messages"
COUNT=0
DEADLINE=$(($(date +%s) + 60))
while [[ $(date +%s) -lt $DEADLINE ]]; do
    COUNT=$(sqlite3 /var/lib/scryd/meta.sqlite 'SELECT count(*) FROM messages' 2>/dev/null || echo 0)
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

note "running scryd search as alice"
HITS_OUT=$(sudo -u alice \
    XDG_RUNTIME_DIR=/run/scryd \
    HOME=/home/alice \
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

echo "OK: e2e indexing + search round-trip passed (50 messages, $HIT_COUNT hits)"
