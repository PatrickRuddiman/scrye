Parent slice: [mcp](../slices/mcp.md)
Depends on: 31

# Task 33 — CLI rewired as MCP client

> **SUPERSEDED — not implemented as written.** A later directive reduced scryd to
> *exactly* fetch + index + MCP-serve with **no other control surface or client**.
> So instead of rewiring the `scryd` CLI into an MCP client, the **CLI client was
> removed entirely**: `scryd` is now a subcommand-less daemon (`scryd/src/main.rs`),
> and `uds_client.rs` / `path_resolution.rs` / `output.rs` / `config_writer.rs` /
> the `cmd_*` tests are deleted. There is no `scryd/src/mcp_client.rs`. Search is
> reachable only over the MCP server; accounts are declared in the config TOML.
> The Tasks/Acceptance below are retained for history but do **not** describe the
> shipped code. See [slices/cli.md](../slices/cli.md) (superseded) and the root
> [README.md](../README.md).

## Goal
The `scryd` CLI talks to the running daemon as an MCP client over `http://<bind>/mcp`; the UDS HTTP client and socket-path resolution are removed, and every verb maps to its MCP tool.

## Tasks
- [ ] Create `scryd/src/mcp_client.rs`: an `rmcp` streamable-HTTP client connecting to `http://<bind>/mcp` (bind resolved from `SCRYD_MCP_BIND`, then the loaded config `server.mcp_bind`, then `127.0.0.1:7878`), with typed `call_tool` helpers, mapping a connection-refused/transport error to the existing `ExitCode::DaemonNotRunning` (see `scryd/src/exit.rs`).
- [ ] Rewire `scryd/src/main.rs`: `run_search` (`scryd/src/main.rs:299`) calls the `search` tool then renders via `output.rs`; `run_status` (`main.rs:190`) calls `status` and prints the JSON; `run_sync` (`main.rs:155`) calls `sync`; `run_reindex` (`main.rs:251`) calls `reindex`, mapping the conflict error to the existing "a reindex is already running" message; the reconcile trigger in the add/rotate/remove flow (`main.rs:570`) calls the `reconcile` tool.
- [ ] Adapt `scryd/src/output.rs` to deserialize the `search` tool's structured result (same hit fields as `SearchHitDto`) instead of the raw HTTP response body.
- [ ] Delete `scryd/src/uds_client.rs` and remove the socket-path resolution in `scryd/src/path_resolution.rs` (and any `mod uds_client;` / socket references in `scryd/src/main.rs`) that is no longer used.
- [ ] In `scryd/Cargo.toml` remove the `scryd-api` dependency, add `rmcp` (client + reqwest streamable-HTTP transport features) and a `scryd-mcp` path dependency if shared DTOs are reused.

## Acceptance criteria
- [ ] `cargo build -p scryd` exits 0.
- [ ] `cargo test -p scryd` passes.
- [ ] `test ! -f scryd/src/uds_client.rs` (the UDS client is gone).
- [ ] `git grep -nE "UdsClient|/internal/reconcile|/internal/reindex|scryd_api|scryd-api" scryd/src scryd/Cargo.toml` matches 0.
- [ ] `git grep -nE "/mcp" scryd/src/mcp_client.rs` matches at least 1.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
