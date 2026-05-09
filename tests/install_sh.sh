#!/usr/bin/env bash
# Privileged smoke test for ops/install.sh and ops/uninstall.sh.
#
# Re-launches itself inside a debian:bookworm Docker container with
# --privileged (needed for systemd-tmpfiles + useradd to behave like
# they would on a real host). Inside the container, runs five
# scenarios end-to-end against the v0.2.0 system installer.

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
cp ops/scryd.tmpfiles.in "$BUNDLE/scryd.tmpfiles.in"
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
(cd "$BUNDLE" && ./install.sh --user alice --skip-weights --skip-systemctl)

[[ -e /etc/scryd/config.toml ]] || fail "scenario 1: /etc/scryd/config.toml missing"
own=$(stat -c '%U:%G %a' /etc/scryd/config.toml)
[[ "$own" == "scryd:scryd 600" ]] || fail "scenario 1: config.toml ownership/mode '$own' != 'scryd:scryd 600'"

[[ -d /var/lib/scryd ]] || fail "scenario 1: /var/lib/scryd missing"
own=$(stat -c '%U:%G %a' /var/lib/scryd)
[[ "$own" == "scryd:scryd 700" ]] || fail "scenario 1: /var/lib/scryd ownership/mode '$own' != 'scryd:scryd 700'"

[[ -e /etc/systemd/system/scryd.service ]] || fail "scenario 1: scryd.service missing"
[[ -e /etc/tmpfiles.d/scryd.conf ]] || fail "scenario 1: scryd.conf tmpfiles drop-in missing"

[[ -d /run/scryd ]] || fail "scenario 1: /run/scryd missing"
own=$(stat -c '%U:%G %a' /run/scryd)
[[ "$own" == "scryd:alice 750" ]] || fail "scenario 1: /run/scryd ownership/mode '$own' != 'scryd:alice 750'"

grep -q '^User=scryd$' /etc/systemd/system/scryd.service || fail "scenario 1: rendered unit missing User=scryd"
alice_uid=$(id -u alice)
grep -q "^Environment=SCRYD_ALLOWED_UID=${alice_uid}$" /etc/systemd/system/scryd.service || \
    fail "scenario 1: rendered unit missing SCRYD_ALLOWED_UID=${alice_uid}"

# ---------- Scenario 2: Isolation property ----------
note "scenario 2: isolation property"
if su mallory -c 'cat /etc/scryd/config.toml' 2>/tmp/mallory.err; then
    fail "scenario 2: mallory was able to read /etc/scryd/config.toml"
fi
grep -q "Permission denied" /tmp/mallory.err || \
    fail "scenario 2: mallory's read failed but stderr didn't say 'Permission denied': $(cat /tmp/mallory.err)"

if su alice -c 'cat /etc/scryd/config.toml' 2>/tmp/alice.err; then
    fail "scenario 2: alice (operator's UID) was able to read /etc/scryd/config.toml"
fi
grep -q "Permission denied" /tmp/alice.err || \
    fail "scenario 2: alice's read failed but stderr didn't say 'Permission denied': $(cat /tmp/alice.err)"

# ---------- Scenario 3: v0.2.x re-install idempotency ----------
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
chmod 0600 /etc/scryd/config.toml
sha_before=$(sha256sum /etc/scryd/config.toml | awk '{print $1}')

(cd "$BUNDLE" && ./install.sh --user alice --skip-weights --skip-systemctl)

sha_after=$(sha256sum /etc/scryd/config.toml | awk '{print $1}')
[[ "$sha_before" == "$sha_after" ]] || \
    fail "scenario 3: config.toml sha changed across re-install: $sha_before -> $sha_after"

# ---------- Scenario 4: Uninstall round-trip ----------
note "scenario 4: uninstall round-trip"
(cd "$BUNDLE" && ./uninstall.sh)

[[ ! -e /etc/systemd/system/scryd.service ]] || fail "scenario 4: scryd.service still present"
[[ ! -e /etc/scryd ]] || fail "scenario 4: /etc/scryd still present"
[[ ! -e /var/lib/scryd ]] || fail "scenario 4: /var/lib/scryd still present"
[[ ! -e /run/scryd ]] || fail "scenario 4: /run/scryd still present"
[[ ! -e /usr/local/bin/scryd ]] || fail "scenario 4: /usr/local/bin/scryd still present"
[[ -z "$(getent passwd scryd || true)" ]] || fail "scenario 4: scryd user not deleted"

(cd "$BUNDLE" && ./uninstall.sh)  # idempotent second run

# ---------- Scenario 5: v0.1.0 -> v0.2.0 transition ----------
note "scenario 5: v0.1.0 detection + --remove-v01-data"
mkdir -p /home/alice/.local/bin /home/alice/.config/systemd/user /home/alice/.config/scryd
touch /home/alice/.local/bin/scryd \
      /home/alice/.local/bin/scryd-fetch-weights \
      /home/alice/.config/systemd/user/scryd.service \
      /home/alice/.config/scryd/config.toml
chown -R alice:alice /home/alice/.local /home/alice/.config

set +e
(cd "$BUNDLE" && ./install.sh --user alice --skip-weights --skip-systemctl) 2>/tmp/v01.err
rc=$?
set -e
[[ "$rc" -eq 2 ]] || fail "scenario 5: install without --remove-v01-data should exit 2, got $rc"
grep -q '/home/alice/.local/bin/scryd' /tmp/v01.err || \
    fail "scenario 5: stderr didn't name the v0.1.0 sentinel: $(cat /tmp/v01.err)"
[[ ! -e /etc/scryd ]] || fail "scenario 5: install bailed but /etc/scryd was created anyway"

(cd "$BUNDLE" && ./install.sh --user alice --skip-weights --skip-systemctl --remove-v01-data)

[[ ! -e /home/alice/.local/bin/scryd ]] || fail "scenario 5: v0.1.0 binary not removed"
[[ ! -e /home/alice/.local/bin/scryd-fetch-weights ]] || fail "scenario 5: v0.1.0 helper not removed"
[[ ! -e /home/alice/.config/systemd/user/scryd.service ]] || fail "scenario 5: v0.1.0 unit not removed"
[[ ! -e /home/alice/.config/scryd/config.toml ]] || fail "scenario 5: v0.1.0 config not removed"

[[ -e /etc/scryd/config.toml ]] || fail "scenario 5: v0.2.0 config.toml not laid down"
[[ -e /etc/systemd/system/scryd.service ]] || fail "scenario 5: v0.2.0 unit not laid down"

echo "OK: install.sh smoke test passed (5 scenarios)"
