Parent slice: [mcp](../slices/mcp.md)
Depends on: 28

# Task 30 — scryd-mcp write tools (scoped)

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Three write tools — `sync`, `reindex` (single-flight), `reconcile` — perform the daemon control actions and return scope-safe structured results, with tests proving scope filtering and the single-flight lock.

## Tasks
- [x] Create `crates/scryd-mcp/src/tools/write.rs` with a `sync` tool porting `crates/scryd-api/src/handlers_write.rs:32` — when a scheduler is present call `request_pass()`, else list active accounts — then filter the returned account list to `scope.allowed_account_ids` and return `{ started: true, scope: { accounts } }`.
- [x] Add a `reindex` tool porting `crates/scryd-api/src/handlers_write.rs:66`: acquire `McpState.reindex_lock` (`Mutex<Option<ReindexHandle>>`); if already `Some`, return a conflict-coded `McpError`; otherwise `indexer.truncate()`, `storage.reenqueue_all_messages()`, store the `ReindexHandle`, and return `{ started: true }`.
- [x] Add a `reconcile` tool porting `crates/scryd-api/src/handlers_write.rs:126`: read the `Arc<RwLock<Config>>`, call `storage.reconcile_from_config(&config)`, then filter `added`/`updated`/`inactivated` to the allowed account ids and return `{ started: true, accounts_added, accounts_updated, accounts_inactivated }`.
- [x] Add the input/output DTOs for the three tools to `crates/scryd-mcp/src/dto.rs` (no input args; structured outputs) and register the tools in the shared `#[tool_router] impl` (the impl that task 31 finalises in `server.rs`).
- [x] Create `crates/scryd-mcp/tests/write_tools.rs`: assert `sync`'s returned scope contains only owned accounts; a second `reindex` call while the lock is held returns the conflict error; `reconcile`'s output account lists exclude foreign account ids.

## Acceptance criteria
- [x] `cargo test -p scryd-mcp --test write_tools` passes (scope filtering + single-flight lock).
- [x] `git grep -nE "fn (sync|reindex|reconcile)\b" crates/scryd-mcp/src` matches 3.
- [x] `cargo build -p scryd-mcp` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
