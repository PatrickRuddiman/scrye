---
sources:
  - crates/scryd-log/src/lib.rs
  - crates/scryd-log/src/categories.rs
  - crates/scryd-log/src/macros.rs
---

# Observability

scryd's runtime signals are structured logs and the MCP
[`status`](../mcp/tools/status.md) tool. This page covers the log format, the
closed sets that tag events, and how secrets are kept out of the logs.

## Log format

scryd logs through `tracing`. On a non-TTY (under systemd/journald) it emits
JSON-lines; on an interactive TTY it emits a pretty, human-readable format. Under
the systemd unit, view logs with:

```
journalctl -u scryd -f
```

Log level is controlled by `RUST_LOG`. When it is unset, scryd applies a default
filter; the exact value is documented with [Configuration](../configuration.md).

## Failure categories

Failure events are tagged with a `category` from this closed set:

| Category string |
| --- |
| `connect failure` |
| `tls failure` |
| `auth rejection` |
| `push-channel drop` |
| `uid-validity reset triggering re-sync` |
| `single-message parse failure` |
| `single-message full-text indexer failure` |
| `single-message semantic indexer failure` |
| `disk-full` |
| `configuration parse error` |
| `configuration permission error` |
| `non-owner-user connection rejection` |

The single-message parse failure category is subtyped by the
[parse faults](mime-and-markdown.md#parse-faults).

## Lifecycle kinds

Non-failure lifecycle events are tagged with a `kind` from this closed set:

| Kind string |
| --- |
| `startup` |
| `shutdown` |
| `sync_pass_start` |
| `sync_pass_complete` |
| `backfill_progress` |
| `idle_enter` |
| `idle_drop` |
| `idle_recycle` |
| `uidvalidity_reset` |
| `reindex_start` |
| `reindex_complete` |
| `account_reconciled` |
| `request` |

## Secrets in logs

Account passwords never reach the logs. A password is held in a secret type whose
debug output is `AccountPassword([REDACTED])` and whose contents are zeroed on
drop, so it cannot be logged by accident. See [Configuration](../configuration.md)
and [Security](../security.md).

## Live status

For point-in-time health rather than a log stream, the MCP
[`status`](../mcp/tools/status.md) tool reports uptime, per-account sync state,
index-queue depth, and the crash-loop supervisor. See
[Daemon lifecycle](lifecycle.md) for the crash-loop model.

## See also

- [Configuration](../configuration.md)
- [Daemon lifecycle](lifecycle.md)
- [IMAP sync](imap-sync.md)
- [MIME and Markdown](mime-and-markdown.md)
- [status tool](../mcp/tools/status.md)
- [Troubleshooting](../operations/troubleshooting.md)
