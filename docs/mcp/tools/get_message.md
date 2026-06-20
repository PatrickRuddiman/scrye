---
sources:
  - crates/scryd-mcp/src/tools/read.rs
  - crates/scryd-mcp/src/dto.rs
---

# get_message

Fetch a single parsed message by its internal id.

## Arguments

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | The `message_id` from a search hit. |

## Response

A message object:

| Field | Type | Notes |
| --- | --- | --- |
| `message_id` | string | Internal id. |
| `account_id` | string | Owning account. |
| `folder` | string | Folder. |
| `header_message_id` | string or null | The RFC 5322 `Message-ID` header. |
| `in_reply_to` | string or null | `In-Reply-To` header. |
| `references` | array of string | Parsed `References` header. |
| `thread_id` | string | Pass to `get_thread`. |
| `from` | object | `{ "addr", "name"? }`. |
| `to` | array | Recipient address objects. |
| `cc` | array | Carbon-copy address objects. |
| `subject` | string or null | Subject. |
| `date` | string | RFC 3339 timestamp. |
| `body_md` | string | Message body rendered to Markdown. |
| `attachments` | array | Always empty in this version; see the note. |

Address objects carry `addr` and an optional `name`. The `body_md` field is the
text/plain body, or the HTML body converted to Markdown — see
[MIME and Markdown](../../concepts/mime-and-markdown.md).

### Attachments

`attachments` is always an empty array in this version: attachment metadata is
not persisted yet. The field is present so callers do not have to special-case
its absence.

## Errors

| Condition | Error |
| --- | --- |
| No such id, tombstoned, or owned by another account | `resource_not_found` |
| Storage failure | `internal_error` |

A message owned by another account returns the same `resource_not_found` as a
missing one, so ids cannot be probed across the scope boundary.

## Example

A `tools/call` request:

```json
{
  "method": "tools/call",
  "params": { "name": "get_message", "arguments": { "id": "<message_id>" } }
}
```

## See also

- [MCP interface](../README.md)
- [get_raw_message](./get_raw_message.md)
- [get_thread](./get_thread.md)
- [MIME and Markdown](../../concepts/mime-and-markdown.md)
