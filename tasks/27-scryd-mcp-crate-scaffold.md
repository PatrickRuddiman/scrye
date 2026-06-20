Parent slice: [mcp](../slices/mcp.md)
Depends on: none

# Task 27 — scryd-mcp crate scaffold + mcp_bind config

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
A new `scryd-mcp` crate compiles with the `rmcp` 1.7 server stack and exposes `McpState`, `DaemonHealthSnapshot`, `ReindexHandle`, and an error type; `[server].mcp_bind` is added to config additively so the existing `scryd-api`/`scryd-runtime` keep compiling.

## Tasks
- [x] Create `crates/scryd-mcp/Cargo.toml` (package `scryd-mcp`, workspace `edition`/`version`) with deps: `rmcp` `1.7` features `["server","macros","schemars","transport-streamable-http-server"]`; a crate-local `axum = "0.8"` (NOT the workspace 0.7 pin); `tokio`, `serde`, `serde_json`, `schemars`, `tokio-util`, `tracing`, `thiserror`; and path deps `scryd-config`, `scryd-storage`, `scryd-search`, `scryd-imap`. Run `cargo tree -p rmcp` and align the axum/hyper versions to whatever `rmcp` resolves.
- [x] Add `"crates/scryd-mcp"` to the workspace `members` array at `Cargo.toml:3`.
- [x] Create `crates/scryd-mcp/src/lib.rs` declaring modules `state` and `error` now, plus empty `scope`, `dto`, `tools`, `server`, `serve` module files (filled by tasks 28–31) so the crate tree compiles.
- [x] Create `crates/scryd-mcp/src/state.rs` porting `ReindexHandle` (from `crates/scryd-api/src/state.rs:16`) and `DaemonHealthSnapshot` (from `crates/scryd-api/src/state.rs:26`, keep `Clone, Copy, Default, Debug`), and defining `McpState` mirroring `AppState` (`crates/scryd-api/src/state.rs:39`) fields — `storage`, `searcher`, `indexer`, `reindex_lock`, `config: Arc<RwLock<Config>>`, `started_at`, `scheduler: Option<Arc<Scheduler>>`, `daemon_health` — with `new`/`with_scheduler` constructors matching `AppState`'s. Leave a TODO comment for the `scope` field that task 28 adds.
- [x] Create `crates/scryd-mcp/src/error.rs` with a `thiserror` enum (e.g. `ConfigError`) that includes a `UserEmailMissing` variant for the mandatory-env failure used by task 28.
- [x] In `crates/scryd-config/src/loader.rs:24` add `pub mcp_bind: String` to `ServerCfg` with `#[serde(default = "default_mcp_bind")]` (a new fn returning `"127.0.0.1:7878"`), and add the field to the `Default for ServerCfg` impl at `crates/scryd-config/src/loader.rs:48`. Do NOT remove `require_peer_uid`/`socket_mode` (task 34 does that) so `scryd-api` still compiles.

## Acceptance criteria
- [x] `test -f crates/scryd-mcp/Cargo.toml && test -f crates/scryd-mcp/src/state.rs`.
- [x] `cargo build -p scryd-mcp` exits 0.
- [x] `cargo test -p scryd-config` passes.
- [x] `cargo build -p scryd-api` exits 0 (additive config change did not break the crate being replaced).
- [x] `git grep -nE "mcp_bind" crates/scryd-config/src/loader.rs` matches at least 2 lines (field + default fn).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
