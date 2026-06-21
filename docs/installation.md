---
sources:
  - ops/install.sh
  - ops/uninstall.sh
  - ops/scryd.service.in
  - ops/nfpm/nfpm.yaml
  - ops/nfpm/scripts/
  - scripts/build-packages.sh
  - .github/actions/build-linux-packages/action.yml
  - crates/scryd-runtime/src/xdg.rs
  - ops/README.install.md
---

# Installation

scryd ships two ways: native packages for the common Linux package managers, and
a tarball with a self-contained installer script. Both target a Linux host with
systemd, on x86_64 or arm64. Both keep runtime state under `/etc/scryd` and
`/var/lib/scryd`; they differ in where the binaries and unit file land and in
whether weights are fetched during installation.

## Choosing a method

| | Native package | Tarball + `install.sh` |
| --- | --- | --- |
| Binaries under | `/usr/bin` | `/usr/local/bin` |
| systemd unit | `/usr/lib/systemd/system` | `/etc/systemd/system` |
| Fetches weights at install | No (daemon fetches on first start) | Yes |
| Starts the service | No | Yes (`enable --now`) |
| Upgrade path | Through the package manager | Re-run `install.sh` |

Native packages fit hosts you manage with apt, dnf, apk, or pacman. The tarball
fits hosts without a package repository, or where you want a one-shot install.

## Native packages

Releases attach one artifact per format and architecture. Install the file that
matches your distribution and CPU:

| Manager | Format | Install command |
| --- | --- | --- |
| apt / dpkg | `.deb` | `sudo apt install ./scryd_<version>_<arch>.deb` |
| dnf / yum / zypper | `.rpm` | `sudo dnf install ./scryd-<version>.<arch>.rpm` |
| apk | `.apk` | `sudo apk add --allow-untrusted ./scryd-<version>-<arch>.apk` |
| pacman | `.pkg.tar.zst` | `sudo pacman -U ./scryd-<version>-<arch>.pkg.tar.zst` |

Every package runs the same maintainer scripts. On install it creates the `scryd`
system user, the `/etc/scryd` and `/var/lib/scryd` trees, and an empty
`/etc/scryd/config.toml` (mode `0640`), then reloads systemd. It does not start
the service — the daemon needs `USER_EMAIL` and an account first — and it does
not fetch weights; the daemon fetches them itself on first start. Continue with
[Getting started](./getting-started.md) to configure and start it.

### Alpine and other musl systems

The binaries are linked against glibc. On a stock musl Alpine system, install the
glibc compatibility shim before starting the service:

```sh
sudo apk add gcompat
```

## Tarball installer

Unpack a release tarball and run the bundled installer as root:

```sh
sudo ./install.sh
```

`install.sh` requires `scryd`, `scryd-fetch-weights`, `scryd.service.in`, and
`LICENSE` beside it. It creates the `scryd` user, lays out the directories,
installs the binaries to `/usr/local/bin`, installs the unit to
`/etc/systemd/system/scryd.service`, fetches the weights into
`/var/lib/scryd/assets`, and runs `systemctl enable --now scryd`. It is
idempotent: re-running it preserves your config, weights, and index while
replacing the binaries and unit. Two test-only flags, `--skip-weights` and
`--skip-systemctl`, suppress the network fetch and the systemd calls.

## File layout

| Path | Package | Tarball |
| --- | --- | --- |
| `scryd` binary | `/usr/bin/scryd` | `/usr/local/bin/scryd` |
| `scryd-fetch-weights` | `/usr/bin/scryd-fetch-weights` | `/usr/local/bin/scryd-fetch-weights` |
| systemd unit | `/usr/lib/systemd/system/scryd.service` | `/etc/systemd/system/scryd.service` |
| Config | `/etc/scryd/config.toml` (`0640`) | `/etc/scryd/config.toml` (`0640`) |
| Data directory | `/var/lib/scryd` (`0700`) | `/var/lib/scryd` (`0700`) |
| Model assets | `/var/lib/scryd/assets` | `/var/lib/scryd/assets` |
| Bundled docs | `/usr/share/doc/scryd/` | — |

Inside `/var/lib/scryd` the daemon keeps `meta.sqlite` (metadata and queues),
`witchcraft.sqlite` (the search index), and `raw/` (the stored RFC 822 source).
The unit sets `XDG_CONFIG_HOME=/etc/scryd` and `XDG_DATA_HOME=/var/lib/scryd`;
scryd resolves these directly, skipping the usual `/scryd` suffix. See
[Configuration](./configuration.md) for how paths resolve when you run the binary
outside the unit.

## First start and weights

The first time the daemon starts it ensures the T5 weight bundle (about 61 MB) is
present in `/var/lib/scryd/assets`, fetching it over HTTPS if needed. With a
native package this happens on first service start; with the tarball it has
already happened during `install.sh`. For air-gapped hosts, stage the weights
yourself — see [Weights and assets](./operations/weights-and-assets.md).

## Upgrades

Upgrade a package install through the package manager; the maintainer scripts
keep your config, data, and the `scryd` user in place and only refresh the
binaries and unit. Upgrade a tarball install by re-running `install.sh` from the
new release. Either way the mailbox index is preserved. See
[Upgrades](./operations/upgrades.md).

## Uninstall

Tarball installs include `uninstall.sh`. Run it as root to remove everything
`install.sh` created — the service, unit, binaries, the `/etc/scryd` and
`/var/lib/scryd` trees, and the `scryd` user. It is destructive: your mailbox
copy and search index are deleted. Back up `/var/lib/scryd` first if you want to
keep them.

For a package install, remove the package with your package manager
(`apt remove scryd`, `dnf remove scryd`, `apk del scryd`, `pacman -R scryd`).
Package removal stops and disables the service and removes the binaries and unit,
but deliberately leaves `/var/lib/scryd` and the `scryd` user in place so a
reinstall keeps your index. Delete `/var/lib/scryd` by hand to discard it.

## See also

- [Getting started](./getting-started.md)
- [Configuration](./configuration.md)
- [Weights and assets](./operations/weights-and-assets.md)
- [Upgrades](./operations/upgrades.md)
- [Release and packaging](./contributing/release-and-packaging.md)
