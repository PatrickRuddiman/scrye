---
sources:
  - crates/scryd-mcp/src/tools/read.rs
  - crates/scryd-mcp/src/dto.rs
  - crates/scryd-storage/src/raw.rs
---

# get_raw_message

Fetch the raw RFC 5322 source of a message by its internal id. This is the
on-disk `.eml` exactly as fetched, before parsing.

## Arguments

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | The `message_id` from a search hit. |

## Response

A raw-message object:

| Field | Type | Notes |
| --- | --- | --- |
| `message_id` | string | Internal id. |
| `raw` | string | The stored RFC 5322 source. Decoded as UTF-8, replacing any invalid bytes. |

The source is read from the file recorded for the message in
`/var/lib/scryd/raw/`. See [Storage](../../concepts/storage.md) for how raw
sources are laid out on disk.

## Errors

| Condition | Error |
| --- | --- |
| No such id, tombstoned, or owned by another account | `resource_not_found` |
| The stored source cannot be read, or a storage failure | `internal_error` |

## Example

A `tools/call` request:

```json
{
  "method": "tools/call",
  "params": { "name": "get_raw_message", "arguments": { "id": "<message_id>" } }
}
```

## See also

- [MCP interface](../README.md)
- [get_message](./get_message.md)
- [Storage](../../concepts/storage.md)
