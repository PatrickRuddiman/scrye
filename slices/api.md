Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — api (SUPERSEDED)

> **This slice is superseded by [mcp.md](mcp.md) and no longer reflects the
> codebase.** The HTTP-over-Unix-socket API it described (the `scryd-api` crate,
> the `/run/scryd/scryd.sock` listener, and the `GET /search` /
> `GET /message/:id` / `POST /sync` / `POST /internal/reindex` routes) has been
> **removed**. scryd's only caller-facing surface is now the loopback MCP server
> in [mcp.md](mcp.md); fetch and index run automatically inside the daemon with
> no remote control surface.

Retained as a stub for traceability. The full original design is in git history
(see the commit that introduced [mcp.md](mcp.md) and retired `scryd-api`).
