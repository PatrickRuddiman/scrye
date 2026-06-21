---
sources:
  - ops/install.sh
  - ops/scryd.service.in
  - crates/scryd-config/src/loader.rs
  - crates/scryd-mcp/src/server.rs
  - tests/mcp_client.py
---

# Getting started

This page takes you from nothing to a running scryd daemon that answers an MCP
search against your mailbox. For the full schema and alternative install methods,
see [Configuration](./configuration.md) and [Installation](./installation.md).

## Prerequisites

- A Linux host with systemd, on x86_64 or arm64.
- One IMAP account you can reach, and an app password for it. scryd logs in with
  read-only verbs and never writes to the mailbox.
- Outbound HTTPS on first start, so the daemon can fetch its model weights
  (about 61 MB). See [Weights and assets](./operations/weights-and-assets.md) for
  air-gapped hosts.

## 1. Install

Use a native package for your distribution, or the tarball installer. The tarball
route, run as root from an unpacked release, lays out the full tree and starts
the service:

```sh
sudo ./install.sh
```

This creates the `scryd` system user, `/etc/scryd`, and `/var/lib/scryd`,
installs the binaries and the systemd unit, fetches the weights, and enables the
service. See [Installation](./installation.md) for the package route and the
exact file layout.

## 2. Configure the mailbox

Edit `/etc/scryd/config.toml` to add the account to serve. The file holds your
IMAP password, so it must not be world-readable; `0640` owned by `scryd:scryd` is
correct.

```toml
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "you@example.com"
password = "your-app-password"
```

`tls` defaults to `true` and the folder list defaults to `["INBOX"]`, so a single
account block is enough to begin. Every key is described in
[Configuration](./configuration.md).

## 3. Select the mailbox with USER_EMAIL

scryd serves exactly one account: the one whose `user` equals the `USER_EMAIL`
environment variable, compared case-insensitively. The daemon refuses to start
until `USER_EMAIL` is set. Add it with a systemd drop-in:

```sh
sudo systemctl edit scryd
```

```ini
[Service]
Environment=USER_EMAIL=you@example.com
```

## 4. Start the daemon

```sh
sudo systemctl restart scryd
journalctl -u scryd -f
```

On first start the daemon fetches weights (if not already present), opens the
mailbox, backfills it, and begins indexing. Follow the journal until you see the
sync and MCP-startup events.

## 5. Verify over MCP

scryd's only search surface is the MCP server at `http://127.0.0.1:7878/mcp`. The
repository ships a dependency-free client used by the end-to-end test. List the
tools, then run a search:

```sh
python3 tests/mcp_client.py http://127.0.0.1:7878/mcp tools
python3 tests/mcp_client.py http://127.0.0.1:7878/mcp search "invoice" --limit 5
```

The first command prints the six tool names; the second prints search hits as
JSON. Any MCP-capable assistant can call the same endpoint.

## See also

- [Configuration](./configuration.md)
- [Installation](./installation.md)
- [MCP interface](./mcp/README.md)
- [Running the daemon](./operations/running.md)
