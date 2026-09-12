#!/bin/sh
# scryd package postremove.
#
# Reload systemd now that the vendor unit is gone. We intentionally leave the
# scryd system user and the /etc/scryd + /var/lib/scryd trees in place so an
# accidental remove (or a remove/reinstall) never destroys an operator's
# credentials, index, or fetched weights. Purge those by hand if you mean it:
#   sudo rm -rf /etc/scryd /var/lib/scryd && sudo userdel scryd
set -eu

if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

exit 0
