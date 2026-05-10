Parent plan: scryd v0.3.1 — service pivot
Depends on: none

# Task 09 — scheduler-completeness

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Close the five known-incomplete bullets in scryd-imap's scheduler/supervisor: dynamic `reconcile`, `request_pass` signaling, multi-folder fan-out, tombstone-scan cadence, and mid-session UIDVALIDITY change handling. After this task, the v0.3.0 placeholders are gone and the documented spec behaviour matches the runtime.

## Tasks
- [ ] In `crates/scryd-imap/src/sink.rs:43-53` (`MessageSink` trait), add `async fn list_local_uids(&self, account_id: &str, folder: &str, uidvalidity: u32) -> Result<Vec<u32>, ClientError>;` so the supervisor can request the local UID set for the tombstone diff.
- [ ] In `crates/scryd-imap/tests/support/test_sink.rs` (`CollectingSink`), implement `list_local_uids` returning the captured `submitted` UIDs that match the filter.
- [ ] In `crates/scryd-runtime/src/sink.rs` (`StorageMessageSink`), implement `list_local_uids` via `storage.list_uids_for(account_id, folder, uidvalidity)` (extend `scryd-storage` if no such helper exists; the messages table already has `account_id` + `folder` + `uidvalidity` columns + the `idx_messages_account` index).
- [ ] In `crates/scryd-imap/src/scheduler.rs:130-145` (the `reconcile`/`request_pass` placeholders), implement:
  - `reconcile(&self) -> Result<(), SchedulerError>`: snapshot the current `handles` map; iterate `self.config.accounts`; spawn supervisors for ids not in handles; for ids in handles but not in config, broadcast a per-supervisor "exit" signal and remove from the map.
  - `request_pass(&self) -> Vec<String>`: iterate `handles`; for each healthy supervisor, send a "fetch now" tick via a per-supervisor `tokio::sync::watch::Sender<u64>` (counter so missed wakes still fire); collect the signaled account ids into the return Vec.
- [ ] In `crates/scryd-imap/src/scheduler.rs:start` (the spawn loop), iterate `account.folders.unwrap_or(["INBOX"])`; spawn one supervisor per (account, folder) pair, bounded by `MAX_CONNECTIONS_PER_ACCOUNT` (5) — folders beyond the cap are logged at WARN and skipped.
- [ ] In `crates/scryd-imap/src/idle.rs:run_idle_loop` and `crates/scryd-imap/src/poll.rs:run_poll_loop`, add a per-loop iteration counter; call `tombstone::scan(...)` every `TOMBSTONE_SCAN_EVERY` (10) iterations BEFORE re-entering the wait. The scan needs `local_uids` from `sink.list_local_uids(...)`; pass them in.
- [ ] In `crates/scryd-imap/src/scheduler.rs::run_account_supervisor::run_one_cycle`, after `examine_meta` returns, compare `meta.uid_validity` against the watermark (read via `sink.list_local_uids` or a new sink helper that returns the stored uidvalidity). On mismatch, call `crate::uidvalidity::handle_change(&mut conn, sink, meta.uid_validity.unwrap())` before `run_initial_backfill`.
- [ ] Extend `crates/scryd-imap/tests/scheduler_live.rs` with three new tests:
  - `reconcile_spawns_new_account_supervisors`: start with one account, call reconcile after editing config to add a second, assert two supervisors active.
  - `request_pass_returns_signaled_account_ids_when_healthy`: start, call request_pass, assert the signaled list contains the account.
  - `multi_folder_account_fans_out_one_supervisor_per_folder`: configure account with `folders = ["INBOX", "Sent"]`, start, assert two supervisor tasks (use TaskList or instrumentation).

## Acceptance criteria
- [ ] `cargo test -p scryd-imap --test scheduler_live` passes (existing 2 + 3 new = 5).
- [ ] `! git grep -F 'Reconcile placeholder' crates/scryd-imap/src/scheduler.rs`.
- [ ] `! git grep -F 'request_pass placeholder' crates/scryd-imap/src/scheduler.rs`.
- [ ] `git grep -nE 'tombstone::scan' crates/scryd-imap/src/idle.rs crates/scryd-imap/src/poll.rs | wc -l` returns at least 2.
- [ ] `git grep -nE 'fn list_local_uids' crates/scryd-imap/src/sink.rs crates/scryd-runtime/src/sink.rs | wc -l` returns 2 (trait + impl).
- [ ] `git grep -nE 'handle_change' crates/scryd-imap/src/scheduler.rs` matches the supervisor call site.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
