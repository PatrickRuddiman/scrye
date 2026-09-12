---
sources:
  - crates/scryd-runtime/src/serve.rs
  - crates/scryd-runtime/src/preflight.rs
  - crates/scryd-log/src/categories.rs
  - crates/scryd-mcp/src/serve.rs
  - crates/scryd-mcp/src/scope.rs
  - ops/scryd.service.in
---

# Troubleshooting

Start with the journal and the [`status`](../mcp/tools/status.md) tool:

```
journalctl -u scryd -f
```

The log categories named below are the closed set documented in
[Observability](../concepts/observability.md).

## The daemon will not start

| Symptom | Cause | Fix |
| --- | --- | --- |
| Exits immediately; log mentions the email scope | `USER_EMAIL` is unset | Set `Environment=USER_EMAIL=you@example.com` in a unit override and restart. See [Running scryd](running.md). |
| `configuration permission error`; permission-invariant exit | The config file has world-permission bits, or the data dir is not mode `0700` | Tighten the config to `0640` (owned by `scryd`) and the data dir to `0700`. See [Configuration](../configuration.md). |
| Exit complaining the bind is not loopback | `SCRYD_MCP_BIND` or `[server].mcp_bind` points at a routable address | Use a loopback address such as `127.0.0.1:7878`. |
| Startup fails fetching weights | The asset bundle is missing and the fetch failed | Check connectivity and the helper path (`SCRYD_FETCH_WEIGHTS_BIN`), or stage assets offline. See [Weights and assets](weights-and-assets.md). |

## Searches return nothing

| Symptom | Cause | Fix |
| --- | --- | --- |
| `list_accounts` is empty; all searches are empty | No configured account's `user` equals `USER_EMAIL` | scryd only serves the mailbox matching `USER_EMAIL`. Align the account's `user` with that address. See [MCP interface](../mcp/README.md). |
| New mail is not yet found | It has not been indexed | Check `status.drainer.queue_depth`; indexing is asynchronous. See [Indexing and search](../concepts/indexing-and-search.md). |
| A specific message never appears | Its index attempts were exhausted | Check `status.drainer.failed_permanent` and `last_index_error`; look for `single-message full-text indexer failure` or `single-message semantic indexer failure` in the log. |

## Sync problems

The per-account `health` field in [`status`](../mcp/tools/status.md) names the
last connection result. Match it to the cause:

| `health` | Log category | Cause |
| --- | --- | --- |
| `auth-rejected` | `auth rejection` | The server rejected the credentials. Fix the password; health clears on the next successful login. |
| `connect-failure` | `connect failure` | The host/port is unreachable. |
| `tls-failure` | `tls failure` | The TLS handshake failed. Check `tls`/`tls_ca_path`. See [Configuration](../configuration.md). |
| `transient` | — | A temporary error; the supervisor retries with backoff. See [IMAP sync](../concepts/imap-sync.md). |

## The daemon keeps restarting

When `status.daemon.in_crash_loop` is `true`, the daemon has seen repeated unclean
exits and is rate-limiting its own restarts with backoff. The `status` tool keeps
reporting `ok: false` so the loop stays visible. Read the journal for the
underlying failure that precedes each exit; the backoff resolves on its own once
the cause is fixed. See [Daemon lifecycle](../concepts/lifecycle.md).

## See also

- [Running scryd](running.md)
- [Configuration](../configuration.md)
- [Observability](../concepts/observability.md)
- [Daemon lifecycle](../concepts/lifecycle.md)
- [status tool](../mcp/tools/status.md)
