---
sources:
  - ops/install.sh
  - ops/uninstall.sh
  - ops/nfpm/scripts/postinstall.sh
  - ops/nfpm/scripts/preremove.sh
  - ops/nfpm/scripts/postremove.sh
  - crates/scryd-storage/src/migrations/mod.rs
  - ops/README.install.md
---

# Upgrades and removal

This page covers upgrading scryd and removing it, for both the native packages
and the tarball installer. See [Installation](../installation.md) for first-time
setup.

## Schema migrations

The on-disk schema is migrated automatically. Migrations are forward-only and
registered in an append-only list; at startup scryd applies any whose version is
newer than what `meta.sqlite` records, each inside a transaction, then records the
new version. Running an already-migrated database applies nothing. There is no
manual migration step and no down-migration; recovery from a bad migration is a
later forward migration.

## Upgrading a package install

Install the new package with the system package manager. On an upgrade:

- The service keeps running across the swap — the pre-removal hook is a no-op
  during a version change.
- The post-install hook is idempotent: it creates the `scryd` user only if
  missing and never overwrites an existing `/etc/scryd/config.toml`.
- Your config, index, and fetched weights are preserved.

Restart to pick up the new binary:

```
sudo systemctl restart scryd
```

## Upgrading a tarball install

Re-run the bundled installer. It is idempotent on an existing host: the binary and
unit are replaced, while the config, weights, and index are preserved, and the
daemon is restarted.

```
sudo ./install.sh
```

## Removing a package

Package removal stops and disables the service and reloads systemd. It
deliberately leaves the `scryd` system user and the `/etc/scryd` and
`/var/lib/scryd` trees in place, so an accidental remove or a remove/reinstall
never destroys your credentials, index, or weights.

To purge that state, do it by hand:

```
sudo rm -rf /etc/scryd /var/lib/scryd && sudo userdel scryd
```

## Removing a tarball install

The bundled `uninstall.sh` is destructive by design: it removes the binaries and
unit and then deletes `/etc/scryd` and `/var/lib/scryd` and the `scryd` user.
Unlike package removal, it does not preserve your config or index. Back up
anything you want to keep first.

## See also

- [Installation](../installation.md)
- [Running scryd](running.md)
- [Weights and assets](weights-and-assets.md)
- [Storage](../concepts/storage.md)
