# Installing scryd

scryd is a single-purpose Linux daemon. It fetches the IMAP mailbox whose login
matches the mandatory `USER_EMAIL`, indexes it with witchcraft, and serves search
over an **MCP server on loopback TCP** (`127.0.0.1:7878` by default). There is no
CLI client and no other control surface — the MCP server is the only way in, and
every response is scoped to `USER_EMAIL`.

The installer creates a dedicated `scryd` Linux user, installs the binary at
`/usr/local/bin/scryd`, lays down the systemd unit, and (unless `--skip-weights`
is passed) downloads the witchcraft asset bundle on the daemon's behalf.

## Quick install

Download the per-arch tarball from GitHub Releases (`x86_64-linux` or
`aarch64-linux`), extract, and run the installer with sudo:

```sh
tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
sudo ./install.sh
```

`install.sh` is non-interactive and idempotent. Re-running it after an upgrade
replaces the binary and unit file but preserves `/etc/scryd/config.toml`,
`/var/lib/scryd/meta.sqlite`, `/var/lib/scryd/witchcraft.sqlite`, and
`/var/lib/scryd/assets/`.

Flags (both test-only):

| Flag | Purpose |
|---|---|
| `--skip-weights` | don't run `scryd-fetch-weights`; daemon will auto-fetch on first start |
| `--skip-systemctl` | don't `systemctl daemon-reload` + `enable --now` |

## Install from a distro package

Each release also ships native packages built with
[nfpm](https://nfpm.goreleaser.com) for both `x86_64` and `aarch64`. Download the
one matching your package manager from the
[Releases page](https://github.com/PatrickRuddiman/scrye/releases) and install it
locally (replace `X.Y.Z` and the arch as needed):

```sh
# Debian / Ubuntu / Mint / Pop!_OS
sudo apt install ./scryd_X.Y.Z_amd64.deb

# Fedora / RHEL / Rocky / Alma  (openSUSE: swap dnf -> zypper)
sudo dnf install ./scryd-X.Y.Z-1.x86_64.rpm

# Arch / Manjaro / EndeavourOS
sudo pacman -U ./scryd-X.Y.Z-1-x86_64.pkg.tar.zst

# Alpine — the binary is glibc, so install gcompat first
sudo apk add gcompat
sudo apk add --allow-untrusted ./scryd_X.Y.Z_x86_64.apk
```

Packages own the system tree, so they differ from the tarball install: the
binaries land in **`/usr/bin`** and the unit at
**`/usr/lib/systemd/system/scryd.service`**. The package creates the `scryd`
user, lays out `/etc/scryd` + `/var/lib/scryd`, and runs `daemon-reload`, but it
does **not** start the service — `USER_EMAIL` is mandatory. After installing,
follow [Configure the mailbox](#configure-the-mailbox) and then
`sudo systemctl enable --now scryd`. The first start fetches the weights
automatically.

Verify a download against the release's `SHA256SUMS` before installing:

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

## What the install creates

The tarball installer (`install.sh`) creates:

| Artifact | Owner | Mode | Notes |
|---|---|---|---|
| `/usr/local/bin/scryd` | root:root | 0755 | the daemon binary (no subcommands) |
| `/usr/local/bin/scryd-fetch-weights` | root:root | 0755 | asset-bundle downloader |
| `/etc/scryd/config.toml` | scryd:scryd | 0640 | IMAP account; group `scryd` can read, world cannot |
| `/var/lib/scryd/` | scryd:scryd | 0700 | meta DB + witchcraft index |
| `/var/lib/scryd/assets/` | scryd:scryd | 0755 | xtr asset bundle (mmap-readable) |
| `/etc/systemd/system/scryd.service` | root:root | 0644 | the unit (no socket, no tmpfiles) |

Native packages create the same `scryd` user, `/etc/scryd`, and `/var/lib/scryd`
tree, but install the binaries under `/usr/bin` and the unit at
`/usr/lib/systemd/system/scryd.service`.


## Service posture (what scryd defends, what it doesn't)

| Operation | Outcome |
|---|---|
| `open("/etc/scryd/config.toml")` from a UID not in group `scryd` | EACCES (mode 0640) |
| `open("/etc/scryd/config.toml")` from a UID in group `scryd` | OK — the credential is readable to group members by design |
| `connect("127.0.0.1:7878")` from another local process | OK — v1 has no per-caller auth; the host is the trust boundary |
| `connect()` from off-host | refused — the MCP listener is loopback-only and rejects routable binds |
| `read("/var/lib/scryd/...")` from a UID not in group `scryd` | EACCES (mode 0700) |
| `ptrace(scryd_pid)` from a UID other than `scryd` | EPERM (different UID, YAMA blocks) |

**The boundary is `USER_EMAIL` + loopback.** The daemon serves a single mailbox
to local MCP clients; it does not authenticate callers. Keep it on a host where
you trust the local processes, and don't expose `127.0.0.1:7878` to a network. See
[`docs/security.md`](../docs/security.md) for the full threat model.

## Configure the mailbox

scryd serves the mailbox named by `USER_EMAIL`, so you must set it **and** add a
matching `[[accounts]]` entry whose `user` equals that address.

1. Set `USER_EMAIL` on the unit (mandatory — the daemon won't start until you do):

   ```sh
   sudo systemctl edit scryd
   # add under [Service]:
   #   Environment=USER_EMAIL=alice@example.com
   ```

2. Hand-edit `/etc/scryd/config.toml` (owned by `scryd:scryd`, mode 0640):

   ```toml
   [[accounts]]
   id = "primary"
   host = "imap.example.com"
   port = 993
   user = "alice@example.com"   # must equal USER_EMAIL
   password = "..."
   tls = true
   folders = ["INBOX"]
   # optional per-account custom CA for self-signed corporate IMAP:
   # tls_ca_path = "/etc/scryd/work-ca.pem"

   # optional: override the loopback MCP bind (default 127.0.0.1:7878)
   # [server]
   # mcp_bind = "127.0.0.1:7878"
   ```

   `port` defaults are not applied for you — set `993` for implicit-TLS IMAP.
   `id` must match `^[a-z0-9_-]+$`. The CA PEM, if used, must be readable by the
   `scryd` user. There is no `add-account` command: the config file is the only
   way to declare the account, and the daemon reads it at start.

3. Start it:

   ```sh
   sudo systemctl restart scryd
   journalctl -u scryd -f
   ```

   Editing the config and restarting is the supported way to change accounts or
   TLS settings; the unit has no `ExecReload` and there is no socket/CLI reconcile
   path.

## Connect an MCP client

Point any MCP client (Claude Desktop, an agent framework, your own code) at the
Streamable-HTTP endpoint:

```
http://127.0.0.1:7878/mcp
```

It advertises six read-only tools: `search`, `get_message`, `get_raw_message`,
`get_thread`, `list_accounts`, `status`. Fetching and indexing happen
automatically inside the daemon — there are no sync/reindex/add-account tools and
no write surface.

A minimal stdlib MCP client used by the e2e test lives at
[`tests/mcp_client.py`](../tests/mcp_client.py); it's a handy way to poke the
server from a shell:

```sh
python3 tests/mcp_client.py http://127.0.0.1:7878/mcp tools
python3 tests/mcp_client.py http://127.0.0.1:7878/mcp search "annual invoice from acme" --limit 10
```

## The `status` tool — readiness contract

`status` is the readiness and progress probe. Its structured result has the shape:

```json
{
  "ok": true,
  "uptime_secs": 1234,
  "accounts": [
    {
      "account_id": "primary",
      "folders": ["INBOX"],
      "health": "Healthy",
      "last_sync_unix": 1731600000,
      "last_seen_uid": 4271
    }
  ]
}
```

`last_seen_uid` is `null` until the IMAP supervisor has fetched at least one
message from the primary folder. `last_sync_unix` is `null` until the first
complete sync pass finishes. `accounts` is empty if no matching `[[accounts]]` is
configured (or none matches `USER_EMAIL`). `ok` is `false` while the daemon is in
crash-loop backoff (issue #20), so a chatty restart loop stays visible instead of
masquerading as healthy.

## Witchcraft asset bundle

On first start the daemon checks `/var/lib/scryd/assets/` for `config.json`,
`tokenizer.json`, and `xtr.gguf`. If any are missing it spawns
`scryd-fetch-weights`, which downloads the tarball baked into the binary at
compile time (~61 MB), verifies its SHA-256, and extracts the three files.
Re-running the helper when all three are present is a no-op.

The bundle is produced by the `witchcraft-assets` GitHub workflow against a
pinned `dropbox/witchcraft` revision and published as a github release tagged
`witchcraft-assets-<short-rev>`.

For air-gapped installs: pre-stage the three files under `/var/lib/scryd/assets/`
(owned by scryd:scryd, mode 0644) and the fetcher's existence check
short-circuits.

## Logs

scryd is a system unit, so the journal is the system journal — no `--user` flag.

```sh
# live tail
journalctl -u scryd -f

# last 200 lines as raw JSON
journalctl -u scryd --output cat -n 200

# filter by failure category
journalctl -u scryd --output json \
    | jq 'select(.MESSAGE | contains("configuration permission error"))'
```

## Uninstall

```sh
sudo ./uninstall.sh
```

Removes the system service, unit file, the FHS directory tree (`/etc/scryd`,
`/var/lib/scryd`), the binaries, and the `scryd` Linux account. Idempotent: a
second run prints `nothing was installed`.

## Upgrade

Re-run the installer:

```sh
sudo ./install.sh
```

The binary and unit are replaced; `/etc/scryd/config.toml`, `/var/lib/scryd/`,
the witchcraft index at `/var/lib/scryd/witchcraft.sqlite`, and the asset bundle
at `/var/lib/scryd/assets/` are preserved. systemd restarts the daemon on the
unit reload.

## glibc requirement

Release tarballs require glibc 2.34 or newer (Ubuntu 22.04+, Debian 12+,
Fedora 36+, Arch). On older distros, build from source against your local
toolchain.
