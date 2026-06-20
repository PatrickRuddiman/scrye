Parent slice: [mcp](../slices/mcp.md)
Depends on: 31

# Task 32 — Runtime serve rewire to MCP

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
`scryd serve` boots the MCP server on loopback TCP instead of the UDS axum API, fails fast when `USER_EMAIL` is unset, and preserves the issue-#20 crash-loop run recording, backoff, and `daemon_health`.

## Tasks
- [ ] In `crates/scryd-runtime/src/serve.rs:10` replace the `scryd_api::{bind, router as api_router, AppState, DaemonHealthSnapshot}` import with `scryd_mcp::{serve as mcp_serve, McpState, DaemonHealthSnapshot, AccountScope}`.
- [ ] In `serve_init` (after `Config::load`, around `crates/scryd-runtime/src/serve.rs:64`), construct `AccountScope::from_env()` and map its error to a fail-fast `RuntimeError` (new `Config`/`UserEmail` variant in `crates/scryd-runtime/src/lib.rs`) so the daemon exits non-zero when `USER_EMAIL` is missing/empty — before `begin_run`.
- [ ] Replace the transport block at `crates/scryd-runtime/src/serve.rs:166-195`: build `McpState` (mirror the `AppState::with_scheduler` call at `serve.rs:168` and set `daemon_health` from `serve.rs:115`, and pass the `AccountScope`), resolve the bind `SocketAddr` from `SCRYD_MCP_BIND` then `api_config.server.mcp_bind`, spawn `mcp_serve(state, addr, shutdown_future)` on a task, and delete the `runtime_dir()` + `bind()` UDS calls.
- [ ] Update `ServeContext` (`crates/scryd-runtime/src/serve.rs:27`) to hold the MCP server `JoinHandle<std::io::Result<()>>` and a shutdown signal (`tokio::sync::watch` sender or `tokio_util::sync::CancellationToken`) in place of `api_handle`/`api_shutdown`; update the struct construction at `serve.rs:199-208` and the teardown order in `serve_run` (`serve.rs:214-230`) to signal MCP shutdown, await the handle, then keep `scheduler.shutdown()` → drainer cancel → `finish_run(run_id)` unchanged.
- [ ] In `crates/scryd-runtime/Cargo.toml` remove the `scryd-api` path dependency and add a `scryd-mcp` path dependency.

## Acceptance criteria
- [ ] `cargo build -p scryd-runtime` exits 0.
- [ ] `cargo test -p scryd-runtime` passes.
- [ ] `git grep -nE "scryd_api|scryd-api" crates/scryd-runtime` matches 0.
- [ ] `git grep -nE "from_env\(\)" crates/scryd-runtime/src/serve.rs` matches at least 1 (USER_EMAIL fail-fast).
- [ ] `git grep -nE "mcp_serve|scryd_mcp::serve" crates/scryd-runtime/src/serve.rs` matches at least 1.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
