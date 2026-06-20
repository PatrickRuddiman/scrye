Parent slice: [mcp](../slices/mcp.md)
Depends on: 29, 30

# Task 31 — Streamable-HTTP transport + ServerHandler

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
The MCP server is reachable over Streamable HTTP on a loopback TCP listener at `/mcp`, advertises the nine tools through `ServerHandler::get_info`, and an `rmcp` client round-trips a scoped tool call in an integration test.

## Tasks
- [x] Create `crates/scryd-mcp/src/server.rs` defining the `McpServer` handler struct holding `McpState` + `tool_router: ToolRouter<McpServer>` with a `new(state)` constructor (`tool_router: Self::tool_router()`); move the `#[tool_router] impl McpServer` block here aggregating the read tools (task 29) and write tools (task 30); and add `#[tool_handler] impl ServerHandler for McpServer` whose `get_info()` returns a `ServerInfo` with `ServerCapabilities::builder().enable_tools().build()`, `Implementation::from_build_env()`, and an instructions string naming the nine tools.
- [x] Create `crates/scryd-mcp/src/serve.rs` exposing `pub async fn serve(state: McpState, bind: std::net::SocketAddr, shutdown: impl std::future::Future<Output = ()> + Send + 'static) -> std::io::Result<()>`: construct `StreamableHttpService::new(move || Ok(McpServer::new(state.clone())), LocalSessionManager::default().into(), StreamableHttpServerConfig::default())`, mount it via `axum::Router::new().nest_service("/mcp", service)`, bind a `tokio::net::TcpListener`, and run `axum::serve(listener, router).with_graceful_shutdown(shutdown)`.
- [x] Re-export `serve`, `McpServer`, `McpState`, `DaemonHealthSnapshot`, and `AccountScope` from `crates/scryd-mcp/src/lib.rs`.
- [x] Add the `rmcp` reqwest streamable-HTTP **client** transport as a `[dev-dependencies]` feature in `crates/scryd-mcp/Cargo.toml` (confirm the exact feature flag with `cargo`).
- [x] Create `crates/scryd-mcp/tests/roundtrip.rs`: build an `McpState` over a temp `StorageHandle` (owned + foreign account), call `serve` bound to `127.0.0.1:0`, read the actual local port, connect an `rmcp` streamable-HTTP client to `http://127.0.0.1:<port>/mcp`, assert `list_tools` returns the nine tool names, call `status` and assert success, and call `get_message` with a foreign-owned id asserting a `not_found` error.

## Acceptance criteria
- [x] `cargo test -p scryd-mcp --test roundtrip` passes (live server on loopback TCP + real `rmcp` client).
- [x] `git grep -nE "nest_service\(.+/mcp" crates/scryd-mcp/src/serve.rs` matches 1.
- [x] `git grep -nE "impl ServerHandler for McpServer" crates/scryd-mcp/src/server.rs` matches 1.
- [x] `cargo build -p scryd-mcp` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
