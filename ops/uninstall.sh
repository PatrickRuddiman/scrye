#!/usr/bin/env bash
# Uninstaller for scryd.
#
# Removes everything install.sh produces — the system service, unit
# file, FHS directory tree, binaries, and the `scryd` Linux account.
# Idempotent: a second run after a clean uninstall succeeds with
# "nothing was installed".

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "scryd uninstall: must run as root (try: sudo $0)" >&2
    exit 1
fi

removed=0

if systemctl status scryd >/dev/null 2>&1 || \
   [[ -e /etc/systemd/system/scryd.service ]]; then
    systemctl disable --now scryd 2>/dev/null || true
    removed=$((removed + 1))
fi

for f in /etc/systemd/system/scryd.service \
         /usr/local/bin/scryd /usr/local/bin/scryd-fetch-weights; do
    if [[ -e "$f" ]]; then
        removed=$((removed + 1))
    fi
done
rm -f /etc/systemd/system/scryd.service /etc/tmpfiles.d/scryd.conf
rm -f /usr/local/bin/scryd /usr/local/bin/scryd-fetch-weights

for d in /etc/scryd /var/lib/scryd; do
    if [[ -e "$d" ]]; then
        removed=$((removed + 1))
    fi
done
rm -rf /etc/scryd /var/lib/scryd

if getent passwd scryd >/dev/null; then
    userdel scryd 2>/dev/null || true
    removed=$((removed + 1))
fi

systemctl daemon-reload 2>/dev/null || true

if [[ $removed -eq 0 ]]; then
    echo "scryd uninstall: nothing was installed."
else
    echo "scryd uninstall: removed $removed artifact(s) — service, unit, tmpfiles, binaries, dirs, system user."
fi
