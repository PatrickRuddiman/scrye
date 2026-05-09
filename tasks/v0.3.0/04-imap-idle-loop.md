Parent plan: GreenMail integration testing for scryd
Parent v0.1.0 task: [14-scryd-imap-idle-poll-tombstone.md](../14-scryd-imap-idle-poll-tombstone.md)
Depends on: 03

# Task 04 — imap-idle-loop

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `run_idle_loop` in `crates/scryd-imap/src/idle.rs` per the v0.1.0 task 14 spec: enter IDLE; `tokio::select!` between IDLE channel events / recycle timer (`IDLE_RECYCLE = 25 min`) / shutdown signal; on EXISTS run `run_incremental`; on recycle send DONE+NOOP; on shutdown send DONE and exit.

## Tasks
- [x] In `crates/scryd-imap/src/idle.rs`, add `pub async fn run_idle_loop(conn: &mut Connection, client: &mut Client<S>, sink: &dyn MessageSink, idle_recycle: Duration, shutdown: tokio::sync::watch::Receiver<bool>) -> Result<(), ClientError>`. Generic over `S: AsyncRead + AsyncWrite + Unpin + Send + Debug`.
- [x] Per-iteration order: `client.idle_start()` → `tokio::select!` between (a) idle channel reporting EXISTS/FETCH/RECENT (drives `run_incremental`), (b) `tokio::time::sleep(idle_recycle)` (sends DONE + NOOP), (c) shutdown.changed() (sends DONE, returns).
- [x] Wire `tombstone::scan` cadence per spec: a counter, every `TOMBSTONE_SCAN_EVERY = 10` iterations call `tombstone::scan` before re-entering IDLE.
- [x] Emit `log_lifecycle!(kind = kind::IDLE_ENTER | kind::IDLE_DROP | kind::IDLE_RECYCLE)` at the relevant transitions (the helpers already exist in `idle.rs:14-42`).
- [x] Add `crates/scryd-imap/tests/idle_live.rs`:
  - skip-if-unreachable; clear_mailbox; inject 3 messages.
  - run_initial_backfill → spawn `run_idle_loop` with `idle_recycle = 200ms` in a `tokio::spawn`.
  - Within 5s budget, inject a 4th message via SMTP and assert it lands in storage.
  - Trigger shutdown via the watch channel; assert task returns Ok.

## Acceptance criteria
- [x] `cargo test -p scryd-imap --test idle_live` passes locally with GreenMail.
- [x] `git grep -nE 'pub async fn run_idle_loop' crates/scryd-imap/src/idle.rs` matches.
- [x] Existing `cargo test -p scryd-imap --test idle` still passes (lifecycle-log unit tests).
- [x] `git grep -F 'run_incremental' crates/scryd-imap/src/idle.rs` matches the EXISTS handler.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
