---
sources:
  - crates/scryd-mcp/src/tools/read.rs
  - crates/scryd-mcp/src/dto.rs
---

# get_thread

Fetch every message in a thread, oldest first. Only messages from your accounts
that have not been tombstoned are returned.

## Arguments

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | A `thread_id`, as returned in a search hit or a message. |

## Response

A thread object:

| Field | Type | Notes |
| --- | --- | --- |
| `thread_id` | string | The thread id you asked for. |
| `messages` | array | Full message objects, oldest first. |

Each entry has the same shape as the [`get_message`](./get_message.md) response.

## Errors

| Condition | Error |
| --- | --- |
| The thread has no messages you own (missing, tombstoned, or all foreign) | `resource_not_found` |
| Storage failure | `internal_error` |

Threads are assembled by following each message's `References` and `In-Reply-To`
headers. See [Storage](../../concepts/storage.md) for how threading is computed.

## Example

A `tools/call` request:

```json
{
  "method": "tools/call",
  "params": { "name": "get_thread", "arguments": { "id": "<thread_id>" } }
}
```

## See also

- [MCP interface](../README.md)
- [get_message](./get_message.md)
- [search](./search.md)
- [Storage](../../concepts/storage.md)
