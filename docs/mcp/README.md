---
sources:
  - crates/scryd-mcp/src/server.rs
  - crates/scryd-mcp/src/serve.rs
  - crates/scryd-mcp/src/scope.rs
  - crates/scryd-mcp/src/error.rs
  - crates/scryd-mcp/src/dto.rs
  - tests/mcp_client.py
---

# MCP interface

scryd's only query surface is a Model Context Protocol (MCP) server. It exposes
six read-only tools over the Streamable HTTP transport, bound to loopback. There
is no other API: fetching, parsing, and indexing run inside the daemon with no
caller-facing controls.

## Endpoint and transport

The server mounts at `/mcp` on the configured loopback address (default
`127.0.0.1:7878`), so the endpoint is `http://127.0.0.1:7878/mcp`. Change the
address with `[server].mcp_bind` or the `SCRYD_MCP_BIND` environment variable;
both must be loopback. See [Configuration](../configuration.md).

The transport is MCP Streamable HTTP: JSON-RPC 2.0 over HTTP POST, with each
response returned as either a JSON body or a `text/event-stream`. A session id is
issued on `initialize` and echoed on later requests through the `Mcp-Session-Id`
header.

## Tools

| Tool | Purpose |
| --- | --- |
| [`search`](./tools/search.md) | Ranked search across your mailbox. |
| [`get_message`](./tools/get_message.md) | One parsed message by id. |
| [`get_raw_message`](./tools/get_raw_message.md) | The raw RFC 5322 source by id. |
| [`get_thread`](./tools/get_thread.md) | Every message in a thread, oldest first. |
| [`list_accounts`](./tools/list_accounts.md) | Your configured accounts. |
| [`status`](./tools/status.md) | Daemon health and per-account sync state. |

## Account scoping

Every response is scoped to the mailbox named by the mandatory `USER_EMAIL`
environment variable. On each call scryd resolves `USER_EMAIL` (lowercased) to the
set of accounts whose IMAP login matches it, case-insensitively, and restricts
the result to those accounts. The scope is re-resolved per call, so adding or
removing an account takes effect without a restart.

The `search` tool's `account_ids` argument can only narrow this owned set, never
widen it. If no configured account matches `USER_EMAIL`, every tool returns an
empty result or `resource_not_found` — never an error that would reveal whether
an id exists. The daemon refuses to start at all when `USER_EMAIL` is unset or
empty.

## Error model

Per-request failures use standard MCP (JSON-RPC) errors:

| Condition | Error |
| --- | --- |
| Unknown `mode`, or `since`/`until` not `YYYY-MM-DD` | `invalid_params` |
| Id missing, tombstoned, or owned by another account | `resource_not_found` |
| Storage, search, or filesystem failure | `internal_error` |

`resource_not_found` is deliberately uniform: a message that exists but belongs
to another account is reported exactly like a missing one, so ids cannot be
probed across the scope boundary.

## Calling the server

The repository ships `tests/mcp_client.py`, a dependency-free Streamable-HTTP
client used by the end-to-end test. It performs the full handshake — `initialize`,
the `notifications/initialized` notification, then `tools/list` or `tools/call`:

```sh
python3 tests/mcp_client.py http://127.0.0.1:7878/mcp tools
python3 tests/mcp_client.py http://127.0.0.1:7878/mcp search "quarterly report" --limit 10
```

Any MCP-capable assistant or library can connect the same way: point it at the
endpoint and call the tools by name.

## See also

- [Getting started](../getting-started.md)
- [Configuration](../configuration.md)
- [Security model](../security.md)
- [Indexing and search](../concepts/indexing-and-search.md)
