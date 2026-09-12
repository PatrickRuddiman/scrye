#!/bin/sh
# scryd package postinstall.
#
# Mirrors ops/install.sh for the native-package install path: create the
# dedicated `scryd` system user, lay out the FHS tree it owns, and reload
# systemd so the vendor unit is visible.
#
# It deliberately does NOT enable/start the service (USER_EMAIL is mandatory and
# the daemon refuses to start until it is set) and does NOT fetch model weights
# (the daemon self-fetches them on first start). Portable across the
# dpkg/rpm/apk/pacman maintainer-script environments, so it is plain POSIX sh
# and assumes only coreutils-or-busybox tools.
set -eu

SCRYD_HOME=/var/lib/scryd

group_exists() {
    getent group "$1" >/dev/null 2>&1 || grep -q "^$1:" /etc/group 2>/dev/null
}

user_exists() {
    id "$1" >/dev/null 2>&1
}

# --- dedicated system group + user (idempotent, tool-agnostic) ---
if ! group_exists scryd; then
    if command -v groupadd >/dev/null 2>&1; then
        groupadd --system scryd
    elif command -v addgroup >/dev/null 2>&1; then
        addgroup -S scryd
    fi
fi

if ! user_exists scryd; then
    if command -v useradd >/dev/null 2>&1; then
        useradd --system --gid scryd --home-dir "$SCRYD_HOME" \
            --no-create-home --shell /usr/sbin/nologin scryd
    elif command -v adduser >/dev/null 2>&1; then
        # busybox/Alpine adduser
        adduser -S -D -H -h "$SCRYD_HOME" -s /sbin/nologin -G scryd scryd
    fi
fi

# --- FHS tree owned by scryd (matches ops/install.sh + README.install.md) ---
install -d -m 0755 -o scryd -g scryd /etc/scryd
install -d -m 0700 -o scryd -g scryd "$SCRYD_HOME"
install -d -m 0755 -o scryd -g scryd "$SCRYD_HOME/assets"

# Create an empty, group-readable config on first install only; never clobber an
# operator's existing credentials on upgrade.
if [ ! -e /etc/scryd/config.toml ]; then
    install -m 0640 -o scryd -g scryd /dev/null /etc/scryd/config.toml
fi

if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

cat <<'EOF'

scryd installed. It will not start until you configure the mailbox to serve:
  1) set USER_EMAIL:
       sudo systemctl edit scryd
       # under [Service]:  Environment=USER_EMAIL=you@example.com
  2) add the matching IMAP account to /etc/scryd/config.toml
  3) enable + start:
       sudo systemctl enable --now scryd

Docs: /usr/share/doc/scryd/README.install.md   (logs: journalctl -u scryd -f)
EOF

exit 0
