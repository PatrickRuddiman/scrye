# Installing scryd

scryd v0.3.1 ships as a Linux system service. The installer creates a
dedicated `scryd` Linux user, installs the binary at
`/usr/local/bin/scryd`, lays down the systemd unit, and (unless
`--skip-weights` is passed) downloads the witchcraft asset bundle on
behalf of the daemon. The HTTP-over-UDS search API is open by default
— anyone who can reach `/run/scryd/scryd.sock` can query the full
index. The consumer's higher-layer API service is the auth boundary;
it authenticates end-users and passes a per-user `account_ids` filter
on every search call.

## Quick install

Download the per-arch tarball from GitHub Releases (`x86_64-linux`
or `aarch64-linux`), extract, and run the installer with sudo:

```sh
tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
sudo ./install.sh
```

`install.sh` is non-interactive and idempotent. Re-running it after
an upgrade replaces the binary and unit file but preserves
`/etc/scryd/config.toml`, `/var/lib/scryd/meta.sqlite`,
`/var/lib/scryd/witchcraft.sqlite`, and `/var/lib/scryd/assets/`.

Flags (both test-only):

| Flag | Purpose |
|---|---|
| `--skip-weights` | don't run `scryd-fetch-weights`; daemon will auto-fetch on first start |
| `--skip-systemctl` | don't `systemctl daemon-reload` + `enable --now` |

## What the install creates

| Artifact | Owner | Mode | Notes |
|---|---|---|---|
| `/usr/local/bin/scryd` | root:root | 0755 | daemon + CLI binary |
| `/usr/local/bin/scryd-fetch-weights` | root:root | 0755 | asset-bundle downloader |
| `/etc/scryd/config.toml` | scryd:scryd | 0640 | IMAP accounts; group `scryd` can read, world cannot |
| `/var/lib/scryd/` | scryd:scryd | 0700 | meta DB + witchcraft index |
| `/var/lib/scryd/assets/` | scryd:scryd | 0755 | xtr asset bundle (mmap-readable) |
| `/run/scryd/` | scryd:scryd | 0755 | runtime dir; provisioned by tmpfiles.d at boot |
| `/run/scryd/scryd.sock` | scryd:scryd | 0666 | API socket (mode is `[server] socket_mode` configurable) |
| `/etc/systemd/system/scryd.service` | root:root | 0644 | rendered from `scryd.service.in` |
| `/etc/tmpfiles.d/scryd.conf` | root:root | 0644 | rendered from `scryd.tmpfiles.in` |

## Service posture (what scryd defends, what it doesn't)

| Operation | Outcome |
|---|---|
| `open("/etc/scryd/config.toml")` from a UID not in group `scryd` | EACCES (mode 0640) |
| `open("/etc/scryd/config.toml")` from a UID in group `scryd` | OK — the credential is readable to group members by design |
| `connect("/run/scryd/scryd.sock")` from any local UID | OK by default (mode 0666); see hardening below |
| `read("/var/lib/scryd/...")` from a UID not in group `scryd` | EACCES (mode 0700) |
| `ptrace(scryd_pid)` from a UID other than `scryd` | EPERM (different UID, YAMA blocks) |

**The search API has no auth.** That's the design: the consumer's
higher-layer API authenticates end-users and passes a per-user
`?account_ids=a,b,c` filter on every search call. Empty filter =
all accounts visible to the daemon.

To tighten the socket, edit `/etc/scryd/config.toml` and set either:

```toml
[server]
socket_mode = 0o660       # restrict to group `scryd` (place
                          # consumer's UID in that group)
# or:
require_peer_uid = true   # only the daemon's own UID can connect
                          # (v0.2.0's peercred behaviour, opt-in)
```

Then `sudo systemctl restart scryd`.

## First account configuration

Interactive:

```sh
sudo scryd add-account
```

The CLI prompts for IMAP host / port / user / password / folders;
writes them into `/etc/scryd/config.toml` and POSTs
`/internal/reconcile` so the daemon picks up the change without a
restart. If the daemon isn't running yet, the CLI prints
`apply changes: sudo systemctl start scryd`.

Non-interactive (provisioning):

```sh
printf '%s' "$IMAP_PASSWORD" | sudo scryd add-account \
    --account-id work \
    --host imap.example.com \
    --port 993 \
    --user alice@example.com \
    --password-stdin \
    --folders INBOX,Archive
```

`--port` defaults to `993`; `--folders` defaults to `INBOX`.
`--account-id` must match `^[a-z0-9_-]+$`. The verb is upsert-
semantic on the id: re-running with the same `--account-id`
overwrites the existing entry, so provisioning scripts and AI
agents can call it unconditionally without checking first.

The minimal hand-edited config:

```toml
[[accounts]]
id = "work"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "..."
tls = true
folders = ["INBOX"]
# optional per-account custom CA for self-signed corporate IMAP.
# tls_ca_path = "/etc/scryd/work-ca.pem"
```

## Daily use

Reads (socket is 0666 by default; no sudo needed):

```sh
scryd search "lunch with bob since:2026-01-01"
scryd search "contract terms" --accounts work,personal --json
scryd reindex
scryd sync
scryd status
```

Mutations (sudo because only scryd:scryd can write the config; each
auto-reconciles via `/internal/reconcile` so no restart is needed):

```sh
sudo scryd add-account
sudo scryd rotate-password <account-id>
sudo scryd remove-account <account-id>
```

## `scryd status` output contract

`scryd status` is the readiness and progress probe — provisioning
scripts and agents lean on it. The contract:

- **Exit 0** iff the daemon is reachable on `/run/scryd/scryd.sock`.
  Non-zero means the daemon isn't running, the socket isn't mode-
  reachable from the calling UID, or the HTTP call returned a non-2xx.
- **Stdout is JSON** with shape:

  ```json
  {
    "ok": true,
    "uptime_secs": 1234,
    "accounts": [
      {
        "account_id": "work",
        "folders": ["INBOX"],
        "health": "Healthy",
        "last_sync_unix": 1731600000,
        "last_seen_uid": 4271
      }
    ]
  }
  ```

  `last_seen_uid` is `null` until the IMAP supervisor has fetched at
  least one message from the primary folder. `last_sync_unix` is
  `null` until the first complete sync pass finishes. `accounts` is
  empty if no `[[accounts]]` are configured or the daemon hasn't
  picked up a freshly-reconciled config yet.

No `--json` flag exists because the body is JSON by default — pipe
straight into `jq` or `python3 -m json.tool`.

## TLS to corporate / self-signed IMAP servers

`tls_ca_path` is **config-only** — `scryd add-account` does not
accept a `--tls-ca-path` flag. For self-signed corporate IMAP, run
`add-account` first, then hand-edit `/etc/scryd/config.toml` to add
the field on the `[[accounts]]` block:

```toml
[[accounts]]
id = "work"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "..."
tls = true
folders = ["INBOX"]
tls_ca_path = "/etc/scryd/work-ca.pem"
```

Then `sudo systemctl restart scryd` (the unit has no `ExecReload`,
and hand-edits don't trigger the `/internal/reconcile` path that
the CLI verbs use). The CA PEM must be readable by the `scryd`
user.

## Witchcraft asset bundle

On first start the daemon checks `/var/lib/scryd/assets/` for
`config.json`, `tokenizer.json`, and `xtr.gguf`. If any are missing
it spawns `scryd-fetch-weights`, which downloads the tarball baked
into the binary at compile time (~61 MB), verifies its SHA-256, and
extracts the three files. Re-running the helper when all three are
present is a no-op.

The bundle is produced by the `witchcraft-assets` GitHub workflow
against a pinned `dropbox/witchcraft` revision (see the workflow
file for which one) and published as a github release tagged
`witchcraft-assets-<short-rev>`.

For air-gapped installs: pre-stage the three files under
`/var/lib/scryd/assets/` (owned by scryd:scryd, mode 0644) and the
fetcher's existence check short-circuits.

## Logs

scryd is a system unit, so the journal is the system journal — no
`--user` flag.

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

Removes the system service, unit file, tmpfiles drop-in, the FHS
directory tree (`/etc/scryd`, `/var/lib/scryd`, `/run/scryd`), the
binaries, and the `scryd` Linux account. Idempotent: a second run
prints `nothing was installed`.

## Upgrade

Re-run the installer:

```sh
sudo ./install.sh
```

The binary and unit are replaced; `/etc/scryd/config.toml`,
`/var/lib/scryd/`, the witchcraft index at
`/var/lib/scryd/witchcraft.sqlite`, and the asset bundle at
`/var/lib/scryd/assets/` are preserved. systemd restarts the daemon
on the unit reload.

## glibc requirement

Release tarballs require glibc 2.34 or newer (Ubuntu 22.04+, Debian 12+,
Fedora 36+, Arch). On older distros, build from source against your
local toolchain.
