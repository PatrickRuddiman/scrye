Parent slice: [mcp](../slices/mcp.md)
Depends on: 27

# Task 28 — USER_EMAIL account scope resolver

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
`AccountScope` turns the mandatory `USER_EMAIL` env var into the set of owned `account_id`s (matched on `accounts.username`, case-insensitive) and provides the guard helpers every tool uses, with unit tests proving foreign accounts are excluded and the env var is mandatory.

## Tasks
- [x] Fill `crates/scryd-mcp/src/scope.rs` with `AccountScope` holding the lowercased email; `new(email: &str) -> Result<Self, ConfigError>` erroring `UserEmailMissing` when the trimmed value is empty; and `from_env() -> Result<Self, ConfigError>` delegating to `new(std::env::var("USER_EMAIL")...)`.
- [x] In `crates/scryd-mcp/src/scope.rs` add `async fn allowed_account_ids(&self, storage: &scryd_storage::StorageHandle) -> anyhow::Result<Vec<String>>` calling `storage.list_active_accounts()` and keeping rows whose `username` equals the email case-insensitively, returning their `account_id`s.
- [x] In `crates/scryd-mcp/src/scope.rs` add pure helpers `is_allowed(allowed: &[String], account_id: &str) -> bool`, `narrow(allowed: &[String], caller: &[String]) -> Vec<String>` (intersection; empty `caller` returns all `allowed`), and `retain_allowed<T>(allowed: &[String], rows: Vec<T>, key: impl Fn(&T) -> &str) -> Vec<T>`.
- [x] Add the `scope: AccountScope` field to `McpState` in `crates/scryd-mcp/src/state.rs` and thread it through the `new`/`with_scheduler` constructors.
- [x] Add tests in `crates/scryd-mcp/src/scope.rs` (`#[cfg(test)]`) for `new` (empty → error, value → ok), `is_allowed`, and `narrow` (caller asking for a foreign id gets it dropped; empty caller → all allowed). Add `crates/scryd-mcp/tests/scope_storage.rs` that opens a temp `StorageHandle`, seeds an owned + a foreign account, and asserts `allowed_account_ids` returns only the owned id (case-insensitive match).

## Acceptance criteria
- [x] `cargo test -p scryd-mcp scope` passes (unit helpers + env-parsing).
- [x] `cargo test -p scryd-mcp --test scope_storage` passes (real temp storage, case-insensitive match, foreign excluded).
- [x] `git grep -nE "fn (allowed_account_ids|is_allowed|narrow|retain_allowed)" crates/scryd-mcp/src/scope.rs` matches 4.
- [x] `cargo build -p scryd-mcp` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
