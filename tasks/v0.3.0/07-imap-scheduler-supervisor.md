Parent plan: GreenMail integration testing for scryd
Parent v0.1.0 task: [15-scryd-imap-scheduler.md](../15-scryd-imap-scheduler.md)
Depends on: 04, 05, 06

# Task 07 — imap-scheduler-supervisor

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `Scheduler::{new,start,reconcile,request_pass,shutdown}` and the `AccountSupervisor` lifecycle loop per the v0.1.0 task 15 spec, exercised end-to-end against GreenMail with two configured accounts.

## Tasks
- [x] In `crates/scryd-imap/src/scheduler.rs`, complete the `Scheduler` struct definition (`storage`, `sink`, `config`, `supervisors: Mutex<HashMap<String, AccountSupervisor>>`).
- [x] Implement `pub async fn new(storage: StorageHandle, sink: Arc<dyn MessageSink>, config: Arc<scryd_config::Config>) -> Self`.
- [x] Implement `pub async fn start(&self) -> Result<(), SchedulerError>`: for each `accounts.active = 1` row in storage, spawn an `AccountSupervisor` task per (account, folder), bounded by `MAX_CONNECTIONS_PER_ACCOUNT = 5`.
- [x] Implement `pub async fn reconcile(&self) -> Result<ReconcileDiff, SchedulerError>`: call `storage.reconcile_from_config(&self.config)`, spawn supervisors for new accounts, signal removed-account supervisors to LOGOUT and exit, re-sync folder lists for changed accounts. Return the `ReconcileDiff`.
- [x] Implement `pub async fn request_pass(&self) -> Vec<String>`: signal every healthy account supervisor to "fetch now if you can"; return signaled account ids (excludes accounts in backoff).
- [x] Implement `pub async fn shutdown(&self) -> Result<(), SchedulerError>`: cooperative cleanup — send LOGOUT on every connection, await supervisors to exit.
- [x] In `crates/scryd-imap/src/supervisor.rs`, implement the `AccountSupervisor` lifecycle loop on top of the existing log helpers (`log_connect_failure!`, `log_tls_failure!`, `log_auth_rejection!`, `log_push_channel_drop!` are already shipped). The loop:
  - Connect via `connect::login(host, port, account.tls, user, password, account_id)` — depends on task 01's `tls: bool`.
  - On success: `client.examine(folder)` → `run_initial_backfill` (if needed) → either `run_idle_loop` or `run_poll_loop` based on capabilities + `account.use_idle`.
  - On error: emit the corresponding `log_failure!`, update `sync_state.account_health` and `backoff_until` via the sink, sleep until `backoff_until` (computed via `backoff_after`, already shipped), then reconnect.
- [x] Add `crates/scryd-imap/tests/scheduler_live.rs`:
  - Configure two GreenMail users (`test1`/`test1`, `test2`/`test2`) — extend the support module with a per-user inject helper.
  - Start the scheduler with both accounts; inject one message into each.
  - Within 5s budget, assert both messages reach storage.
  - Flip account-1's password to a wrong value via config edit + `reconcile`; force a reconnect; assert account-1's `account_health = "auth-rejected"` while account-2 stays `active` and no failure logs are emitted for account-2.

## Acceptance criteria
- [x] `cargo test -p scryd-imap --test scheduler_live` passes locally with GreenMail.
- [x] `git grep -nE 'pub async fn start|pub async fn reconcile|pub async fn request_pass|pub async fn shutdown' crates/scryd-imap/src/scheduler.rs | wc -l` returns at least 4.
- [x] Existing `cargo test -p scryd-imap --test scheduler` still passes (backoff math + credential fetcher unit tests).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
