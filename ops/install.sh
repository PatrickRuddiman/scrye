#!/usr/bin/env bash
# System installer for scryd v0.2.0.
#
# Lays out the FHS tree (/etc/scryd, /var/lib/scryd, /run/scryd),
# creates a dedicated `scryd` system user, renders the systemd unit
# from scryd.service.in (substituting __UID__ for the operator's uid
# and __USER__ for the operator's login), provisions the runtime dir
# via tmpfiles.d, fetches T5 weights as the scryd user, and enables
# the system service.
#
# Idempotent on a v0.2.x host (config + weights + index preserved;
# binary + unit replaced; daemon restarted). For v0.1.0 hosts pass
# --remove-v01-data to remove the per-user install before laying down
# v0.2.0 — there is no in-place migration.
#
# Flags:
#   --user <name>           operator login (default: $SUDO_USER)
#   --remove-v01-data       wipe v0.1.0 per-user installs before
#                           proceeding
#   --skip-weights          test-only: skip scryd-fetch-weights
#   --skip-systemctl        test-only: skip systemd interactions

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "scryd install: must run as root (try: sudo $0 $*)" >&2
    exit 1
fi

ALLOWED_USER="${SUDO_USER:-}"
REMOVE_V01_DATA=0
SKIP_WEIGHTS=0
SKIP_SYSTEMCTL=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user)
            ALLOWED_USER="${2:-}"
            shift 2
            ;;
        --remove-v01-data)
            REMOVE_V01_DATA=1
            shift
            ;;
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

if [[ -z "$ALLOWED_USER" ]]; then
    echo "scryd install: --user <name> required when SUDO_USER is unset" >&2
    exit 1
fi

ALLOWED_UID="$(getent passwd "$ALLOWED_USER" | awk -F: '{print $3}')"
if [[ -z "$ALLOWED_UID" ]]; then
    echo "scryd install: user '$ALLOWED_USER' does not exist on this host" >&2
    exit 1
fi

V01_FINDINGS=()
V01_FILES=()
detect_v01_users() {
    local user home
    while IFS=: read -r user _ uid _ _ home _; do
        if (( uid < 1000 || uid >= 65534 )); then
            continue
        fi
        local found_for_user=0
        for f in "$home/.local/bin/scryd" \
                 "$home/.local/bin/scryd-fetch-weights" \
                 "$home/.config/systemd/user/scryd.service" \
                 "$home/.config/scryd/config.toml"; do
            if [[ -e "$f" ]]; then
                V01_FILES+=("$f")
                found_for_user=1
            fi
        done
        if [[ $found_for_user -eq 1 ]]; then
            V01_FINDINGS+=("$user:$home")
        fi
    done < <(getent passwd)
}

detect_v01_users

if [[ ${#V01_FINDINGS[@]} -gt 0 && $REMOVE_V01_DATA -eq 0 ]]; then
    echo "scryd install: detected v0.1.0 per-user install on this host:" >&2
    for entry in "${V01_FINDINGS[@]}"; do
        echo "  - ${entry%%:*}  (home: ${entry#*:})" >&2
    done
    for f in "${V01_FILES[@]}"; do
        echo "    $f" >&2
    done
    cat >&2 <<'EOF'

scryd v0.2.0 ships a different on-disk layout than v0.1.0: a single
system daemon at /usr/local/bin/scryd, system unit at
/etc/systemd/system/scryd.service, config under /etc/scryd, data
under /var/lib/scryd. There is no in-place migration of the v0.1.0
index; the operator re-runs `sudo scryd add-account` and the daemon
resyncs from IMAP.

To proceed, re-run with --remove-v01-data, which will:
  - stop and disable each user's `systemctl --user scryd`
  - remove the per-user binary, unit, config and data directories
EOF
    exit 2
fi

if [[ ${#V01_FINDINGS[@]} -gt 0 && $REMOVE_V01_DATA -eq 1 ]]; then
    for entry in "${V01_FINDINGS[@]}"; do
        u="${entry%%:*}"
        h="${entry#*:}"
        echo "scryd install: removing v0.1.0 install for $u (home: $h)"
        sudo -u "$u" systemctl --user stop scryd 2>/dev/null || true
        sudo -u "$u" systemctl --user disable scryd 2>/dev/null || true
        rm -f "$h/.local/bin/scryd" "$h/.local/bin/scryd-fetch-weights"
        rm -f "$h/.config/systemd/user/scryd.service"
        rm -rf "$h/.config/scryd" "$h/.local/share/scryd"
    done
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
for required in scryd scryd-fetch-weights scryd.service.in scryd.tmpfiles.in LICENSE; do
    if [[ ! -e "$SCRIPT_DIR/$required" ]]; then
        echo "scryd install: bundled file missing: $SCRIPT_DIR/$required" >&2
        exit 1
    fi
done

if ! getent passwd scryd >/dev/null; then
    useradd --system --shell /usr/sbin/nologin --home /var/lib/scryd scryd
fi

install -d -m 0700 -o scryd -g scryd /etc/scryd
install -d -m 0700 -o scryd -g scryd /var/lib/scryd
install -d -m 0755 -o scryd -g scryd /var/lib/scryd/assets

if [[ ! -e /etc/scryd/config.toml ]]; then
    install -m 0600 -o scryd -g scryd /dev/null /etc/scryd/config.toml
fi

install -m 0755 "$SCRIPT_DIR/scryd" /usr/local/bin/scryd
install -m 0755 "$SCRIPT_DIR/scryd-fetch-weights" /usr/local/bin/scryd-fetch-weights

sed -e "s/__UID__/$ALLOWED_UID/g" -e "s/__USER__/$ALLOWED_USER/g" \
    "$SCRIPT_DIR/scryd.service.in" > /etc/systemd/system/scryd.service
chmod 0644 /etc/systemd/system/scryd.service

sed -e "s/__USER__/$ALLOWED_USER/g" \
    "$SCRIPT_DIR/scryd.tmpfiles.in" > /etc/tmpfiles.d/scryd.conf
chmod 0644 /etc/tmpfiles.d/scryd.conf

if ! command -v systemd-tmpfiles >/dev/null 2>&1; then
    echo "scryd install: systemd-tmpfiles not found — is this host running systemd?" >&2
    exit 1
fi
systemd-tmpfiles --create /etc/tmpfiles.d/scryd.conf

if [[ $SKIP_WEIGHTS -eq 0 ]]; then
    sudo -u scryd /usr/local/bin/scryd-fetch-weights --target /var/lib/scryd/assets/
    if [[ -e /var/lib/scryd/assets/xtr-weights.gguf ]]; then
        chmod 0644 /var/lib/scryd/assets/xtr-weights.gguf
    fi
fi

if [[ $SKIP_SYSTEMCTL -eq 0 ]]; then
    if ! command -v systemctl >/dev/null 2>&1; then
        echo "scryd install: systemctl not found — is this host running systemd?" >&2
        exit 1
    fi
    systemctl daemon-reload
    systemctl enable --now scryd
fi

cat <<EOF

scryd v0.2.0 installed for operator '$ALLOWED_USER' (uid $ALLOWED_UID).

next steps:
  sudo scryd add-account
  sudo systemctl restart scryd

logs:
  journalctl -u scryd -f

uninstall:
  sudo $SCRIPT_DIR/uninstall.sh
EOF
