---
sources:
  - crates/scryd-mcp/src/tools/read.rs
  - crates/scryd-mcp/src/dto.rs
  - crates/scryd-mcp/src/scope.rs
---

# list_accounts

List your configured accounts. Takes no arguments. Only accounts owned by the
`USER_EMAIL` mailbox are returned.

## Arguments

None.

## Response

An accounts object:

| Field | Type | Notes |
| --- | --- | --- |
| `accounts` | array | One entry per owned account. |

Each account:

| Field | Type | Notes |
| --- | --- | --- |
| `account_id` | string | The account's configured `id`. |
| `folders` | array of string | Folders configured for the account. |

If no configured account matches `USER_EMAIL`, `accounts` is empty.

## Errors

| Condition | Error |
| --- | --- |
| Storage failure | `internal_error` |

## Example

A `tools/call` request:

```json
{
  "method": "tools/call",
  "params": { "name": "list_accounts", "arguments": {} }
}
```

```json
{
  "accounts": [
    { "account_id": "primary", "folders": ["INBOX", "Archive"] }
  ]
}
```

## See also

- [MCP interface](../README.md)
- [status](./status.md)
- [Configuration](../../configuration.md)
