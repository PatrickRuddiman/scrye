#!/usr/bin/env bash
# Privileged smoke test for ops/install.sh and ops/uninstall.sh.
#
# Re-launches itself inside a debian:bookworm Docker container with
# --privileged (needed for useradd to behave like it would on a real
# host). Inside the container, runs four scenarios end-to-end against
# the system installer.

set -euo pipefail

if [[ ! -e /.dockerenv && -z "${SCRYD_SMOKE_INNER:-}" ]]; then
    REPO="$(git rev-parse --show-toplevel)"
    exec docker run --privileged --rm \
        -e SCRYD_SMOKE_INNER=1 \
        -v "$REPO:/workspace" \
        -w /workspace \
        debian:bookworm \
        bash tests/install_sh.sh
fi

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

note() { echo "--- $* ---"; }

note "installing prerequisites"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -qq -y --no-install-recommends \
    systemd sudo passwd coreutils >/dev/null

note "building fixture bundle"
BUNDLE="$(mktemp -d)"
cp ops/install.sh "$BUNDLE/install.sh"
cp ops/uninstall.sh "$BUNDLE/uninstall.sh"
cp ops/scryd.service.in "$BUNDLE/scryd.service.in"
cp LICENSE "$BUNDLE/LICENSE"
chmod +x "$BUNDLE/install.sh" "$BUNDLE/uninstall.sh"

cat > "$BUNDLE/scryd" <<'STUB'
#!/usr/bin/env bash
echo "fake-scryd $@"
STUB
chmod +x "$BUNDLE/scryd"

cat > "$BUNDLE/scryd-fetch-weights" <<'STUB'
#!/usr/bin/env bash
echo "fake-fetch-weights $@"
STUB
chmod +x "$BUNDLE/scryd-fetch-weights"

note "creating test users alice + mallory"
useradd -m alice
useradd -m mallory

# ---------- Scenario 1: Fresh install ----------
note "scenario 1: fresh install"
(cd "$BUNDLE" && ./install.sh --skip-weights --skip-systemctl)

[[ -e /etc/scryd/config.toml ]] || fail "scenario 1: /etc/scryd/config.toml missing"
own=$(stat -c '%U:%G %a' /etc/scryd/config.toml)
[[ "$own" == "scryd:scryd 640" ]] || fail "scenario 1: config.toml ownership/mode '$own' != 'scryd:scryd 640'"

[[ -d /var/lib/scryd ]] || fail "scenario 1: /var/lib/scryd missing"
own=$(stat -c '%U:%G %a' /var/lib/scryd)
[[ "$own" == "scryd:scryd 700" ]] || fail "scenario 1: /var/lib/scryd ownership/mode '$own' != 'scryd:scryd 700'"

[[ -e /etc/systemd/system/scryd.service ]] || fail "scenario 1: scryd.service missing"

grep -q '^User=scryd$' /etc/systemd/system/scryd.service || fail "scenario 1: unit missing User=scryd"
# Daemon-only entrypoint: ExecStart must invoke the bare binary (no `serve`
# subcommand — the CLI surface is gone) and declare the mandatory USER_EMAIL
# scope knob.
grep -q '^ExecStart=/usr/local/bin/scryd$' /etc/systemd/system/scryd.service || \
    fail "scenario 1: unit ExecStart must be the bare 'scryd' daemon (no subcommand)"
grep -q '^Environment=USER_EMAIL=' /etc/systemd/system/scryd.service || \
    fail "scenario 1: unit missing Environment=USER_EMAIL= scope knob"
! grep -q 'XDG_RUNTIME_DIR' /etc/systemd/system/scryd.service || \
    fail "scenario 1: MCP daemon must NOT carry XDG_RUNTIME_DIR (no socket)"
! grep -q 'scryd.sock' /etc/systemd/system/scryd.service || \
    fail "scenario 1: MCP daemon must NOT reference a Unix socket"

# ---------- Scenario 2: config cred-file isolation ----------
note "scenario 2: config is daemon-owned and unreadable by other users"
# alice and mallory are both NOT in the scryd group: neither can
# read /etc/scryd/config.toml (mode 0640 scryd:scryd).
if su mallory -c 'cat /etc/scryd/config.toml' 2>/tmp/mallory.err; then
    fail "scenario 2: mallory was able to read /etc/scryd/config.toml"
fi
grep -q "Permission denied" /tmp/mallory.err || \
    fail "scenario 2: mallory's read failed but stderr didn't say 'Permission denied': $(cat /tmp/mallory.err)"

if su alice -c 'cat /etc/scryd/config.toml' 2>/tmp/alice.err; then
    fail "scenario 2: alice was able to read /etc/scryd/config.toml"
fi
grep -q "Permission denied" /tmp/alice.err || \
    fail "scenario 2: alice's read failed but stderr didn't say 'Permission denied': $(cat /tmp/alice.err)"

# ---------- Scenario 3: re-install idempotency ----------
note "scenario 3: re-install preserves config"
cat > /tmp/extra.toml <<'EOF'

[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "hunter2"
EOF
cat /tmp/extra.toml >> /etc/scryd/config.toml
chown scryd:scryd /etc/scryd/config.toml
chmod 0640 /etc/scryd/config.toml
sha_before=$(sha256sum /etc/scryd/config.toml | awk '{print $1}')

(cd "$BUNDLE" && ./install.sh --skip-weights --skip-systemctl)

sha_after=$(sha256sum /etc/scryd/config.toml | awk '{print $1}')
[[ "$sha_before" == "$sha_after" ]] || \
    fail "scenario 3: config.toml sha changed across re-install: $sha_before -> $sha_after"

# ---------- Scenario 4: Uninstall round-trip ----------
note "scenario 4: uninstall round-trip"
(cd "$BUNDLE" && ./uninstall.sh)

[[ ! -e /etc/systemd/system/scryd.service ]] || fail "scenario 4: scryd.service still present"
[[ ! -e /etc/scryd ]] || fail "scenario 4: /etc/scryd still present"
[[ ! -e /var/lib/scryd ]] || fail "scenario 4: /var/lib/scryd still present"
[[ ! -e /usr/local/bin/scryd ]] || fail "scenario 4: /usr/local/bin/scryd still present"
[[ -z "$(getent passwd scryd || true)" ]] || fail "scenario 4: scryd user not deleted"

(cd "$BUNDLE" && ./uninstall.sh)  # idempotent second run

echo "OK: install.sh smoke test passed (4 scenarios)"
