---
sources:
  - crates/scryd-config/src/loader.rs
  - crates/scryd-config/src/permissions.rs
  - crates/scryd-config/src/secret.rs
  - crates/scryd-runtime/src/xdg.rs
  - crates/scryd-runtime/src/serve.rs
  - crates/scryd-log/src/lib.rs
  - crates/scryd-mcp/src/scope.rs
  - ops/scryd.service.in
---

# Configuration

scryd reads one TOML file and a small set of environment variables. The file
describes the IMAP account to index and a few sync and indexing defaults; the
environment selects which mailbox to serve and where state lives. This page is
the single reference for every configuration key.

## File location

Under the systemd unit, `XDG_CONFIG_HOME=/etc/scryd` is set, and scryd resolves
the config to `/etc/scryd/config.toml`. Running the binary directly, the path
resolves as:

1. `$XDG_CONFIG_HOME/scryd/config.toml`, or, when `XDG_CONFIG_HOME` already ends
   in `scryd`, `$XDG_CONFIG_HOME/config.toml`.
2. Otherwise `$HOME/.config/scryd/config.toml`.
3. If neither variable is set, startup fails with an unresolvable-path error.

## File permissions

The config holds an IMAP password, so scryd refuses to read a world-accessible
file. On load it checks the mode and rejects any file with world bits set
(`mode & 0o007 != 0`). Use `0640` owned by `scryd:scryd`: the daemon and the
`scryd` group can read it, others cannot. `0644` is rejected. This check applies
on Unix and is a no-op on other platforms.

## `[server]`

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `mcp_bind` | string | `"127.0.0.1:7878"` | Address the MCP server binds. Must parse as a socket address and be a loopback address, or the daemon exits at startup. `SCRYD_MCP_BIND` overrides it. |

## `[sync]`

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `folders` | array of string | `["INBOX"]` | Default folder list. An account with no `folders` of its own syncs these. |
| `poll_interval_seconds` | integer | `300` | Parsed and validated; see the note below. |
| `use_idle` | boolean | `true` | Parsed and validated; see the note below. |

`folders` is the fallback folder set: each account syncs its own `folders` if
present, otherwise this list. If both are empty for an account, loading fails.

The current daemon connects to each account and folder and runs an IMAP IDLE
loop, recycling it about every 25 minutes; it does not read
`poll_interval_seconds` or `use_idle`. A polling fallback exists in the code and
is tested, but it is not wired into the running supervisor in this version. Set
these keys for forward compatibility, but expect IDLE behavior regardless. See
[IMAP sync](./concepts/imap-sync.md).

## `[indexers]`

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `semantic` | boolean | `true` | Parsed and validated. The witchcraft engine always builds both the full-text and semantic indexes, so setting this `false` does not currently disable semantic indexing. See [Indexing and search](./concepts/indexing-and-search.md). |

## `[[accounts]]`

Define one account block per mailbox you might serve. Only the account whose
`user` matches `USER_EMAIL` is fetched and indexed; the others are inert.

| Key | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `id` | string | yes | — | Stable identifier. Must match `^[a-z0-9_-]+$` and be unique across accounts. Used in stored-mail paths and MCP account scoping. |
| `host` | string | yes | — | IMAP server hostname. |
| `port` | integer | yes | — | 1–65535. `0` is rejected. Use `993` for implicit TLS. |
| `user` | string | yes | — | IMAP login. Matched case-insensitively against `USER_EMAIL` to choose the served account. |
| `password` | string | yes | — | App password. Held as a secret: zeroed on drop and never logged (its debug form is `AccountPassword([REDACTED])`). |
| `folders` | array of string | no | `[sync].folders` | Folders to sync for this account. Overrides `[sync].folders`. Must not be empty unless `[sync].folders` is non-empty. |
| `tls` | boolean | no | `true` | Use implicit TLS. This selects TLS or plaintext; it is not a certificate-verification switch. Leave `true` except against a local plaintext test server. |
| `tls_ca_path` | path | no | — | PEM file of root certificate(s) to trust for this account instead of the bundled roots. Must be readable by the `scryd` user. |

## Validation errors

`Config::load` fails fast with one of a fixed set of errors:

| Error | Cause |
| --- | --- |
| Not found | No file at the resolved config path. |
| Read | The file exists but cannot be read. |
| Parse | The file is not valid TOML for this schema. |
| Invalid account id | An `id` does not match `^[a-z0-9_-]+$`. |
| Invalid port | An account `port` is `0`. |
| No folders | An account has no folders and `[sync].folders` is empty. |
| Duplicate account id | Two accounts share an `id`. |
| Permission invariant | The config file has world permission bits set. |
| Xdg unresolvable | Neither `XDG_CONFIG_HOME` nor `HOME` is set. |

An empty file (no `[[accounts]]`) loads successfully, but the daemon then has no
account matching `USER_EMAIL` and will not serve until you add one.

## Environment variables

| Variable | Required | Purpose |
| --- | --- | --- |
| `USER_EMAIL` | yes | Selects the single mailbox to serve. Compared case-insensitively to each account's `user`. The daemon refuses to start if it is unset or matches no account. It is set only through the environment, never in the config file. |
| `SCRYD_MCP_BIND` | no | Overrides `[server].mcp_bind`. Must be a loopback socket address. |
| `XDG_CONFIG_HOME` | no | Base directory for the config path. The unit sets it to `/etc/scryd`. |
| `XDG_DATA_HOME` | no | Base directory for state (`meta.sqlite`, `witchcraft.sqlite`, `assets/`, `raw/`). The unit sets it to `/var/lib/scryd`. |
| `RUST_LOG` | no | Log filter, using tracing `EnvFilter` syntax. When unset, scryd applies `scryd=info,scryd_mcp=info,scryd_imap=info,scryd_search=info,scryd_storage=info,scryd_mime=info,scryd_runtime=info,warn`. |

Set `USER_EMAIL` and any overrides on the service with a drop-in:

```sh
sudo systemctl edit scryd
```

```ini
[Service]
Environment=USER_EMAIL=you@example.com
```

## Full example

```toml
[server]
mcp_bind = "127.0.0.1:7878"

[sync]
folders = ["INBOX", "Archive"]

[indexers]
semantic = true

[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "you@example.com"
password = "your-app-password"
tls = true
```

With this file and `USER_EMAIL=you@example.com`, scryd serves the `primary`
account, syncing `INBOX` and `Archive`.

## See also

- [Getting started](./getting-started.md)
- [Installation](./installation.md)
- [IMAP sync](./concepts/imap-sync.md)
- [Indexing and search](./concepts/indexing-and-search.md)
- [Security model](./security.md)
