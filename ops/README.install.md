# Installing scryd

scryd v0.2.0 runs as a dedicated `scryd` Linux system user. The
operator's account talks to the daemon over a Unix-domain socket but
never holds the IMAP credential on disk; only the daemon's UID can
read `/etc/scryd/config.toml`. Agents and scripts running under the
operator's UID share that property — they can search, but they cannot
read the password.

## Quick install

Download the per-arch tarball from GitHub Releases (`x86_64-linux`
or `aarch64-linux`), extract, and run the installer with sudo:

```sh
tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
sudo ./install.sh
```

By default `install.sh` resolves the operator from `$SUDO_USER`. To
install for a different user (e.g. provisioning a fresh host as a
human admin for a service account):

```sh
sudo ./install.sh --user alice
```

## What the install creates

| Artifact | Owner | Mode | Notes |
|---|---|---|---|
| `/usr/local/bin/scryd` | root:root | 0755 | daemon + CLI binary |
| `/usr/local/bin/scryd-fetch-weights` | root:root | 0755 | weights downloader |
| `/etc/scryd/config.toml` | scryd:scryd | 0600 | IMAP accounts (operator can't read) |
| `/var/lib/scryd/` | scryd:scryd | 0700 | meta DB + index |
| `/var/lib/scryd/assets/` | scryd:scryd | 0755 | T5 weights (mmap, world-readable) |
| `/run/scryd/` | scryd:&lt;operator&gt; | 0750 | runtime dir; provisioned by tmpfiles.d at boot |
| `/etc/systemd/system/scryd.service` | root:root | 0644 | rendered from `scryd.service.in` |
| `/etc/tmpfiles.d/scryd.conf` | root:root | 0644 | rendered from `scryd.tmpfiles.in` |

## Isolation properties

| Operation | Operator (`alice`) | Other Linux user (`mallory`) |
|---|---|---|
| `open("/etc/scryd/config.toml")` | EACCES (different UID, mode 0600) | EACCES |
| `connect("/run/scryd/scryd.sock")` | OK (group `alice`, mode 0660) | EACCES (not in the runtime-dir group) |
| `ptrace(scryd_pid)` | EPERM (different UID, YAMA blocks) | EPERM |
| `read("/var/lib/scryd/...")` | EACCES (mode 0700) | EACCES |

`alice`'s shell, MCP servers, and LLM agents share `alice`'s UID so
they get the same row as `alice`: socket access yes, config access
no. That is the property "agents cannot reach the credential."

## First account configuration

```sh
sudo scryd add-account
sudo systemctl restart scryd
```

If the daemon was not yet running, the CLI prints
`apply changes: sudo systemctl start scryd` instead.

## Daily use

Reads (no sudo needed; the operator's UID is allowed on the socket):

```sh
scryd search "lunch with bob since:2026-01-01"
scryd reindex
```

Mutations (sudo because only the daemon's UID can write the config;
each mutation prints a restart hint):

```sh
sudo scryd add-account
sudo scryd rotate-password <account-id>
sudo scryd remove-account <account-id>
sudo systemctl restart scryd
```

## Logs

scryd is a system unit, so the journal is the system journal — no
`--user` flag.

```sh
# live tail
journalctl -u scryd -f

# last 200 lines as raw JSON
journalctl -u scryd --output cat -n 200

# filter by failure category (categories are listed in the spec)
journalctl -u scryd --output json \
  | jq 'select(.MESSAGE | contains("non-owner-user connection rejection"))'
```

## Uninstall

```sh
sudo ./uninstall.sh
```

Removes the system service, unit file, tmpfiles drop-in, the FHS
directory tree (`/etc/scryd`, `/var/lib/scryd`, `/run/scryd`), the
binaries, and the `scryd` Linux account. Idempotent: a second run
prints `nothing was installed`.

## Upgrade

Re-run the installer:

```sh
sudo ./install.sh
```

`install.sh` is idempotent. The binary and unit are replaced;
`/etc/scryd/config.toml`, `/var/lib/scryd/`, and the witchcraft
index at `/var/lib/scryd/witchcraft.sqlite` are preserved. systemd
restarts the daemon on the unit reload.

## Multi-tenant deploys

scryd is open by default — anyone who can reach the socket can
query the full index. The consumer's higher-layer API service is
the auth boundary: it authenticates end-users, decides which
`account_ids` each is allowed to see, and passes that filter
(`?account_ids=a,b,c`) on every search call. Empty filter = all
accounts. See `docs/security.md` for the full posture.

If you want kernel-level access control on the socket as
defense-in-depth, set `[server] socket_mode = 0o660` in
`/etc/scryd/config.toml` and place the consumer's user in the
`scryd` group; or set `[server] require_peer_uid = true` to
recover v0.2.0's same-uid-only posture.

## glibc requirement

Release tarballs require glibc 2.34 or newer (Ubuntu 22.04+, Debian 12+,
Fedora 36+, Arch). On older distros, build from source against your
local toolchain.
