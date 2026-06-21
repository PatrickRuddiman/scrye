#!/bin/sh
# scryd package preremove.
#
# Stop and disable the service, but only on a real removal -- not while a package
# manager is swapping one version for another. The upgrade signal differs per
# packager, so detect both forms that nfpm passes through:
#   * dpkg  prerm: first arg is "upgrade" during an upgrade
#   * rpm   %preun: first arg is "1" when one instance will remain (upgrade)
# apk pre-deinstall and pacman pre_remove only run on actual removal, so they
# fall through to the stop below.
set -eu

case "${1:-}" in
    upgrade|1)
        exit 0
        ;;
esac

if command -v systemctl >/dev/null 2>&1; then
    systemctl disable --now scryd >/dev/null 2>&1 || true
fi

exit 0
