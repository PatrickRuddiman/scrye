Parent plan: GreenMail integration testing for scryd
Parent v0.1.0 task: [14-scryd-imap-idle-poll-tombstone.md](../14-scryd-imap-idle-poll-tombstone.md)
Depends on: 03

# Task 05 — imap-poll-loop

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `run_poll_loop` in `crates/scryd-imap/src/poll.rs` per the v0.1.0 task 14 spec: sleep `poll_interval` → run `run_incremental` → repeat; cooperative shutdown via the shared watch channel; tombstone-scan cadence shared with the IDLE loop.

## Tasks
- [ ] In `crates/scryd-imap/src/poll.rs`, add `pub async fn run_poll_loop(conn: &mut Connection, client: &mut Client<S>, sink: &dyn MessageSink, poll_interval: Duration, shutdown: tokio::sync::watch::Receiver<bool>) -> Result<(), ClientError>`. Generic over `S`.
- [ ] Use `effective_poll_interval` (already implemented at `poll.rs:13-20`) so a configured interval below the floor is clamped.
- [ ] Per-iteration: `tokio::select!` between (a) `tokio::time::sleep(interval)`, (b) `shutdown.changed()`. After sleep, `run_incremental(conn, client, sink).await?`. On shutdown, return Ok.
- [ ] Wire `tombstone::scan` cadence: same `TOMBSTONE_SCAN_EVERY = 10` rule as IDLE loop.
- [ ] Add `crates/scryd-imap/tests/poll_live.rs`:
  - skip-if-unreachable; clear_mailbox; inject 3 messages.
  - run_initial_backfill → spawn `run_poll_loop` with `poll_interval = 100ms` (or the floor-clamped value).
  - Within 1s budget, inject a 4th message and assert it lands in storage.
  - Trigger shutdown; assert task returns Ok.

## Acceptance criteria
- [ ] `cargo test -p scryd-imap --test poll_live` passes locally with GreenMail.
- [ ] `git grep -nE 'pub async fn run_poll_loop' crates/scryd-imap/src/poll.rs` matches.
- [ ] Existing `cargo test -p scryd-imap --test poll` still passes (effective_poll_interval unit tests).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
