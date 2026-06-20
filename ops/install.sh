#!/usr/bin/env bash
# System installer for scryd.
#
# Lays out the FHS tree (/etc/scryd, /var/lib/scryd), creates a
# dedicated `scryd` system user, installs the systemd unit, fetches
# T5 weights, and enables the system service.
#
# scryd is a single-purpose daemon: it fetches the IMAP mailbox whose
# login matches the mandatory USER_EMAIL, indexes it with witchcraft,
# and serves search over an MCP server on loopback TCP (default
# 127.0.0.1:7878). There is no CLI client and no other control surface.
#
# Idempotent on an existing host (config + weights + index preserved;
# binary + unit replaced; daemon restarted).
#
# Flags:
#   --skip-weights          test-only: skip scryd-fetch-weights
#   --skip-systemctl        test-only: skip systemd interactions

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "scryd install: must run as root (try: sudo $0 $*)" >&2
    exit 1
fi

SKIP_WEIGHTS=0
SKIP_SYSTEMCTL=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-weights)
            SKIP_WEIGHTS=1
            shift
            ;;
        --skip-systemctl)
            SKIP_SYSTEMCTL=1
            shift
            ;;
        *)
            echo "scryd install: unknown flag: $1" >&2
            exit 1
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
for required in scryd scryd-fetch-weights scryd.service.in LICENSE; do
    if [[ ! -e "$SCRIPT_DIR/$required" ]]; then
        echo "scryd install: bundled file missing: $SCRIPT_DIR/$required" >&2
        exit 1
    fi
done

if ! getent passwd scryd >/dev/null; then
    useradd --system --shell /usr/sbin/nologin --home /var/lib/scryd scryd
fi

install -d -m 0755 -o scryd -g scryd /etc/scryd
install -d -m 0700 -o scryd -g scryd /var/lib/scryd
install -d -m 0755 -o scryd -g scryd /var/lib/scryd/assets

if [[ ! -e /etc/scryd/config.toml ]]; then
    install -m 0640 -o scryd -g scryd /dev/null /etc/scryd/config.toml
fi

install -m 0755 "$SCRIPT_DIR/scryd" /usr/local/bin/scryd
install -m 0755 "$SCRIPT_DIR/scryd-fetch-weights" /usr/local/bin/scryd-fetch-weights

# The systemd unit ships with no substitution placeholders — copy it
# verbatim. The .in suffix is kept for backwards compatibility with
# operators who may have scripts assuming the rendered-from-template
# shape.
install -m 0644 -o root -g root "$SCRIPT_DIR/scryd.service.in" /etc/systemd/system/scryd.service

if [[ $SKIP_SYSTEMCTL -eq 0 ]]; then
    if ! command -v systemctl >/dev/null 2>&1; then
        echo "scryd install: systemctl not found — is this host running systemd?" >&2
        exit 1
    fi
fi

if [[ $SKIP_WEIGHTS -eq 0 ]]; then
    sudo -u scryd /usr/local/bin/scryd-fetch-weights --target /var/lib/scryd/assets/
    # The bundle extracts to a handful of named files; widen their
    # mode so non-scryd processes can read them at search time.
    for f in config.json tokenizer.json xtr.gguf; do
        if [[ -e /var/lib/scryd/assets/$f ]]; then
            chmod 0644 /var/lib/scryd/assets/$f
        fi
    done
fi

if [[ $SKIP_SYSTEMCTL -eq 0 ]]; then
    if ! command -v systemctl >/dev/null 2>&1; then
        echo "scryd install: systemctl not found — is this host running systemd?" >&2
        exit 1
    fi
    systemctl daemon-reload
    systemctl enable --now scryd
fi

# Report what was actually installed — `scryd --version` emits
# `scryd <semver>`. Strip the binary name so the trailing message
# never drifts from the bundled binary.
INSTALLED_VERSION="$(/usr/local/bin/scryd --version 2>/dev/null | awk 'NR==1{print $2}')"
INSTALLED_VERSION="${INSTALLED_VERSION:-unknown}"

cat <<EOF

scryd v${INSTALLED_VERSION} installed.

next steps:
  1. set the mailbox to serve (mandatory — the daemon won't start until you do):
       sudo systemctl edit scryd
       # add under [Service]:
       #   Environment=USER_EMAIL=you@example.com
  2. add the matching IMAP account to /etc/scryd/config.toml
     ([[accounts]] with user = "you@example.com")
  3. start it:
       sudo systemctl restart scryd
  4. point an MCP client at the search surface:
       http://127.0.0.1:7878/mcp   (override via Environment=SCRYD_MCP_BIND=)

scryd has no CLI client: fetch + index run automatically and search is
served only over MCP.

logs:
  journalctl -u scryd -f

uninstall:
  sudo $SCRIPT_DIR/uninstall.sh
EOF
