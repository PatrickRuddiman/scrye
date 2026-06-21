---
sources:
  - crates/scryd-mcp/src/tools/read.rs
  - crates/scryd-mcp/src/dto.rs
  - crates/scryd-mcp/src/scope.rs
---

# search

Ranked search across your mailbox. Returns the best-matching messages, scoped to
your accounts.

## Arguments

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `q` | string | `""` | Free-text query. Empty matches everything, subject to the filters below. |
| `from` | string | — | Keep only hits whose sender address contains this substring (case-insensitive). |
| `since` | string | — | Inclusive lower date bound, `YYYY-MM-DD` (UTC). |
| `until` | string | — | Inclusive upper date bound, `YYYY-MM-DD` (UTC); expanded to end-of-day. |
| `folder` | string | — | Keep only hits in this exact folder. |
| `account_ids` | array of string | — | Narrow to these account ids. May only intersect your owned accounts, never widen them. |
| `limit` | integer | `20` | Maximum hits, clamped to `1`–`200`. |
| `mode` | string | `"fulltext"` | One of `fulltext`, `semantic`, `hybrid`. See [Indexing and search](../../concepts/indexing-and-search.md). |

## Response

A search response object:

| Field | Type | Notes |
| --- | --- | --- |
| `hits` | array | Ranked hits, best first, at most `limit`. |
| `mode` | string | The mode actually used. |
| `elapsed_ms` | integer | Server-side query time in milliseconds. |

Each hit:

| Field | Type | Notes |
| --- | --- | --- |
| `message_id` | string | Internal id; pass to `get_message` or `get_raw_message`. |
| `account_id` | string | Owning account. |
| `folder` | string | Folder the message is in. |
| `sender_addr` | string | Sender email address. |
| `sender_name` | string or null | Display name, if any. |
| `subject` | string or null | Subject, if any. |
| `date` | string | RFC 3339 timestamp. |
| `score` | number | Relevance score. |
| `snippet` | string | Matched excerpt, up to 240 characters. |
| `thread_id` | string | Pass to `get_thread`. |

## Filtering and scope

The query runs against the index, then each candidate row is re-checked against
`from`, `folder`, `account_ids`, and the `since`/`until` window before it counts
toward `limit`. Tombstoned (deleted-at-source) messages are skipped. If your
owned-account set is empty, or `account_ids` names only accounts you do not own,
the response is an empty `hits` list.

## Errors

| Condition | Error |
| --- | --- |
| `mode` is not `fulltext`/`semantic`/`hybrid` | `invalid_params` |
| `since` or `until` is not `YYYY-MM-DD` | `invalid_params` |
| Storage or search failure | `internal_error` |

## Example

```sh
python3 tests/mcp_client.py http://127.0.0.1:7878/mcp search "quarterly report" --limit 5 --mode hybrid
```

```json
{
  "hits": [
    {
      "message_id": "…",
      "account_id": "primary",
      "folder": "INBOX",
      "sender_addr": "alice@example.com",
      "sender_name": "Alice",
      "subject": "Q3 report",
      "date": "2024-10-01T09:12:00+00:00",
      "score": 12.5,
      "snippet": "… the quarterly report is attached …",
      "thread_id": "…"
    }
  ],
  "mode": "hybrid",
  "elapsed_ms": 7
}
```

## See also

- [MCP interface](../README.md)
- [Indexing and search](../../concepts/indexing-and-search.md)
- [get_message](./get_message.md)
- [get_thread](./get_thread.md)
