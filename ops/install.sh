#!/usr/bin/env bash
# Per-user installer for scryd.
#
# Copies the binary + helper to ~/.local/bin/, the systemd user unit to
# ~/.config/systemd/user/, fetches T5 weights to $XDG_DATA_HOME/scryd/
# assets/, and prints the post-install steps.
#
# This script must run as the user who will own the scryd instance.
# Run as root would put files under /root/.local/bin and break the
# per-user-isolation model the spec relies on.
#
# Test-only env vars (do NOT set in production):
#   SCRYD_INSTALL_SKIP_SYSTEMCTL=1   skip systemctl --user daemon-reload
#                                    (CI / non-systemd environments)
#   SCRYD_INSTALL_SKIP_WEIGHTS=1     skip scryd-fetch-weights download
#                                    (offline test runs)

set -euo pipefail

if [[ $EUID -eq 0 ]] && [[ -z "${SCRYD_INSTALL_ALLOW_ROOT:-}" ]]; then
    echo "scryd install: run as your own user, not root (SCRYD_INSTALL_ALLOW_ROOT only for tests)" >&2
    exit 1
fi

HOME_DIR="${HOME:-}"
if [[ -z "$HOME_DIR" ]]; then
    echo "scryd install: \$HOME is unset" >&2
    exit 1
fi

XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME_DIR/.config}"
XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME_DIR/.local/share}"

# Resolve bundled artifacts: relative to the install.sh script's own dir.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRYD_BIN="$SCRIPT_DIR/scryd"
FETCH_BIN="$SCRIPT_DIR/scryd-fetch-weights"
UNIT_FILE="$SCRIPT_DIR/scryd.service"

for required in "$SCRYD_BIN" "$FETCH_BIN" "$UNIT_FILE"; do
    if [[ ! -e "$required" ]]; then
        echo "scryd install: bundled file missing: $required" >&2
        exit 1
    fi
done

BIN_DIR="$HOME_DIR/.local/bin"
UNIT_DIR="$HOME_DIR/.config/systemd/user"
CONFIG_DIR="$XDG_CONFIG_HOME/scryd"
DATA_DIR="$XDG_DATA_HOME/scryd"
ASSETS_DIR="$DATA_DIR/assets"

mkdir -p "$BIN_DIR"
mkdir -p "$UNIT_DIR"
mkdir -p -m 0700 "$CONFIG_DIR"
mkdir -p -m 0700 "$DATA_DIR"
mkdir -p -m 0700 "$ASSETS_DIR"

# Tighten permissions on dirs that mkdir -m may have left looser if they
# already existed.
chmod 0700 "$CONFIG_DIR" "$DATA_DIR" "$ASSETS_DIR"

install -m 0755 "$SCRYD_BIN" "$BIN_DIR/scryd"
install -m 0755 "$FETCH_BIN" "$BIN_DIR/scryd-fetch-weights"
install -m 0644 "$UNIT_FILE" "$UNIT_DIR/scryd.service"

# Warn if ~/.local/bin isn't in PATH.
case ":${PATH:-}:" in
    *":$BIN_DIR:"*) ;;
    *)
        echo
        echo "scryd install: warning: $BIN_DIR is not in your \$PATH"
        echo "  add this to your shell rc: export PATH=\"\$HOME/.local/bin:\$PATH\""
        ;;
esac

# Fetch weights (skippable for offline / test runs).
if [[ -z "${SCRYD_INSTALL_SKIP_WEIGHTS:-}" ]]; then
    "$BIN_DIR/scryd-fetch-weights" --target "$ASSETS_DIR"
fi

# Reload systemd's user manager so the new unit is visible.
if [[ -z "${SCRYD_INSTALL_SKIP_SYSTEMCTL:-}" ]]; then
    if ! command -v systemctl >/dev/null 2>&1; then
        echo "scryd install: systemctl not found — is this host running systemd?" >&2
        exit 1
    fi
    systemctl --user daemon-reload
fi

cat <<EOF

scryd installed.

next steps:
  scryd add-account
  systemctl --user enable --now scryd

optional (survive logout / start at boot):
  loginctl enable-linger \$USER

logs:
  journalctl --user -u scryd -f
EOF
