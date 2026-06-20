Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — cli (SUPERSEDED)

> **This slice is superseded by [mcp.md](mcp.md) and no longer reflects the
> codebase.** The operator-facing command-line client it described
> (`add-account`, `reindex`, `search`, and the dispatch-over-Unix-socket
> machinery) has been **removed**. The `scryd` binary is now a daemon with **no
> subcommands**: it parses only `--help`/`--version`, then fetches + indexes +
> serves MCP. There is no CLI client; search is reached only over the loopback
> MCP server in [mcp.md](mcp.md), and accounts are declared in the on-disk config
> TOML (read at start), not via a CLI verb.

Retained as a stub for traceability. The full original design is in git history.
