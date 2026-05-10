Parent plan: scryd v0.3.1 — service pivot
Depends on: 09

# Task 10 — cli-sync-status-autoreconcile

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Add two CLI verbs (`scryd sync` + `scryd status`) backed by api endpoints, and have `scryd add-account` POST `/internal/reconcile` after the file write so the operator no longer has to manually `sudo systemctl restart scryd` to pick up a new account.

## Tasks
- [ ] In `crates/scryd-api/src/handlers.rs`, add `pub async fn handle_status(State(state): State<AppState>) -> Response` returning `{ ok: true, data: { uptime_secs, accounts: [{ id, health, last_sync_unix, server_uidnext_seen }, ...] } }`. Pull uptime from a `started_at: Instant` stashed in `AppState` at construction; per-account info from `state.storage.list_active_accounts()` joined with `state.storage.read_sync_state(account_id, "INBOX")` (or whichever folder).
- [ ] In `crates/scryd-api/src/state.rs:21-30` (`AppState`), add `pub started_at: std::time::Instant`; initialise to `Instant::now()` in `AppState::new`.
- [ ] In `crates/scryd-api/src/router.rs:18-32` (the `Router::new()` chain), add `.route("/status", get(handlers::handle_status))`.
- [ ] In `crates/scryd-api/src/handlers_write.rs:31-58` (`handle_sync`), wire the actual scheduler signal: take a `scheduler: Arc<Scheduler>` field on `AppState` (or thread it through some other way), call `scheduler.request_pass().await`, return the signaled account ids in `SyncScopeDto.accounts`. The current "fall back to listing active accounts from storage" path goes away.
- [ ] In `crates/scryd-api/src/state.rs`, add `pub scheduler: Option<Arc<scryd_imap::Scheduler>>` (Option because tests construct `AppState` without a scheduler); update `AppState::new` to take an optional scheduler argument.
- [ ] In `crates/scryd-runtime/src/serve.rs`, plumb the constructed `Arc<Scheduler>` into `AppState::new`.
- [ ] In `scryd/src/main.rs`, add to the `Verb` enum:
  - `Sync` (no args) — POSTs `/sync`, prints the signaled account list.
  - `Status` (no args) — GETs `/status`, prints uptime + per-account health + last_sync_unix in a 2-line block per account.
- [ ] In `scryd/src/main.rs::run_add_account` (after the existing `chown_to_scryd` + `print_restart_hint` calls), POST `/internal/reconcile` if the daemon is reachable; on success replace the restart hint with `account '<id>' picked up by daemon (no restart needed)`. On `DaemonNotRunning`, keep the existing `apply changes: sudo systemctl start scryd` hint.
- [ ] Add `scryd/tests/cmd_sync.rs` with two tests: `sync_against_no_daemon_exits_daemon_not_running` (no fake server → exit 2), `sync_against_running_daemon_prints_signaled_accounts` (fake server returns `{ "started": true, "scope": { "accounts": ["a", "b"] } }` → stdout contains `a` and `b`).
- [ ] Add `scryd/tests/cmd_status.rs` mirroring the same two-test shape.
- [ ] Update `scryd/tests/cmd_add_account.rs::add_account_with_root_writes_config_and_prints_restart_hint`: when the fake reconcile server returns 202, expect the new `picked up by daemon` line instead of the restart hint.

## Acceptance criteria
- [ ] `cargo test -p scryd --test cmd_sync --test cmd_status --test cmd_add_account` passes.
- [ ] `cargo test -p scryd-api --test handlers_read` passes (new /status test if added).
- [ ] `target/debug/scryd sync --help 2>&1 | grep -F 'sync'` matches.
- [ ] `target/debug/scryd status --help 2>&1 | grep -F 'status'` matches.
- [ ] `git grep -nE 'fn handle_status' crates/scryd-api/src/handlers.rs` matches.
- [ ] `git grep -nE 'scheduler.request_pass' crates/scryd-api/src/handlers_write.rs` matches the wired call site.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
