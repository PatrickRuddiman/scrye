---
sources:
  - crates/scryd-mcp/src/tools/read.rs
  - crates/scryd-mcp/src/dto.rs
---

# status

Report daemon health: uptime, per-account sync state, the index drainer, and the
crash-loop supervisor. Takes no arguments. Accounts are scoped to your mailbox.

## Arguments

None.

## Response

A status object:

| Field | Type | Notes |
| --- | --- | --- |
| `ok` | boolean | Overall health. `true` only when the daemon is not in a crash loop. |
| `uptime_secs` | integer | Seconds since the MCP server started. |
| `accounts` | array | Per-account sync state (owned accounts only). |
| `drainer` | object | Index-queue health. |
| `daemon` | object | Crash-loop supervisor state. |

Each `accounts` entry:

| Field | Type | Notes |
| --- | --- | --- |
| `account_id` | string | The account's id. |
| `folders` | array of string | Configured folders. |
| `health` | string or null | Last connection health. One of the closed set listed in [IMAP sync](../../concepts/imap-sync.md#account-health). |
| `last_sync_unix` | integer or null | Unix time of the last full sync. |
| `last_seen_uid` | integer or null | Highest IMAP UID seen in the primary folder. |

`drainer`:

| Field | Type | Notes |
| --- | --- | --- |
| `queue_depth` | integer | Messages waiting to be indexed. |
| `failed_permanent` | integer | Messages that exhausted their index retries. |
| `last_index_error` | object or null | `{ "message_id", "error", "attempts" }` for the most recent failure. |

`daemon`:

| Field | Type | Notes |
| --- | --- | --- |
| `restart_count` | integer | Total recorded daemon starts. |
| `consecutive_crashes` | integer | Crashes in the current crash-loop window. |
| `last_crash_unix` | integer or null | Unix time of the last crash. |
| `in_crash_loop` | boolean | Whether the crash-loop threshold has tripped. |
| `in_backoff` | boolean | Whether the daemon is currently waiting out a restart backoff. |
| `backoff_until_unix` | integer or null | Unix time the current backoff ends. |

`ok` reflects `in_crash_loop`, not the moment of the request: a daemon that is
crash-looping reports `ok: false` even while one request happens to succeed. See
[Daemon lifecycle](../../concepts/lifecycle.md) for the crash-loop model and
[Observability](../../concepts/observability.md) for the health fields.

## Errors

| Condition | Error |
| --- | --- |
| Storage failure | `internal_error` |

## Example

A `tools/call` request:

```json
{
  "method": "tools/call",
  "params": { "name": "status", "arguments": {} }
}
```

## See also

- [MCP interface](../README.md)
- [Daemon lifecycle](../../concepts/lifecycle.md)
- [Observability](../../concepts/observability.md)
- [Troubleshooting](../../operations/troubleshooting.md)
