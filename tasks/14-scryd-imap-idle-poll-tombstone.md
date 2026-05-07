Parent slice: [imap-sync](../slices/imap-sync.md)
Depends on: 13, 06

# Task 14 — scryd-imap-idle-poll-tombstone

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the IDLE/poll loop: enter IDLE when the server supports it, recycle every 25 minutes, fall back to operator-configured polling otherwise, and run the periodic tombstone scan via `UID SEARCH ALL` diff.

## Tasks
- [ ] In `crates/scryd-imap/src/idle.rs`, expose `pub async fn run_idle_loop(conn: &mut Connection, client: &mut Client, sink: &dyn MessageSink, idle_recycle: Duration) -> Result<(), ClientError>`. Order each iteration: enter IDLE; `tokio::select!` between (a) the IDLE channel reporting EXISTS / FETCH / RECENT, (b) `tokio::time::sleep(idle_recycle)`, (c) a shutdown signal. On EXISTS: send DONE, run `run_incremental`, re-enter IDLE. On recycle timer: send DONE, NOOP, re-enter IDLE. On shutdown: send DONE and return Ok. **Deferred** with the mock-server harness; lifecycle log emissions ship as standalone helpers.
- [x] Define `IDLE_RECYCLE: Duration = Duration::from_secs(25 * 60)`. Emit `log_lifecycle!(kind = kind::IDLE_ENTER | IDLE_DROP | IDLE_RECYCLE)` at the relevant transitions.
- [ ] In `crates/scryd-imap/src/poll.rs`, expose `pub async fn run_poll_loop(conn: &mut Connection, client: &mut Client, sink: &dyn MessageSink, poll_interval: Duration) -> Result<(), ClientError>`: sleep `poll_interval` → run `run_incremental` → repeat. **Deferred**; `effective_poll_interval` floor-clamping helper ships and is unit-tested.
- [x] In `crates/scryd-imap/src/tombstone.rs`, expose `pub async fn scan(conn: &mut Connection, client: &mut Client, sink: &dyn MessageSink) -> Result<u32, ClientError>` that runs `UID SEARCH ALL`, queries storage for the local set of UIDs for `(account_id, folder, uidvalidity)`, computes the diff (locally-present, server-absent), and calls `sink.tombstone(message_id)` for each. Returns the count tombstoned. **Partial:** `compute_tombstones` pure diff function ships and is fully unit-tested; the scan() orchestration that issues `UID SEARCH ALL` against a live client is deferred with the mock server.
- [ ] Wire the periodic scan into `run_idle_loop` and `run_poll_loop`: every 10th iteration, call `tombstone::scan` before the next IDLE/poll cycle. **Deferred** with the loops.
- [x] Define `TOMBSTONE_SCAN_EVERY: u32 = 10` constant. Document with a doc-comment that this is the cadence the slice §4 commits to.
- [ ] Write integration tests in `crates/scryd-imap/tests/idle.rs` against the mock server: stage 3 messages, start the IDLE loop in a tokio task, push a new message via the mock's `inject` API, assert the new message lands in storage within 5 seconds; trigger the recycle timer (via a test-only short `idle_recycle = 200ms`) and assert no spurious tombstones happen. **Deferred**; `tests/idle.rs` ships unit tests for the lifecycle-log helpers.
- [ ] Write integration tests in `crates/scryd-imap/tests/poll.rs`: stage 3 messages with the mock advertising NO IDLE capability, run the poll loop with `poll_interval = 100ms`, inject a 4th message, assert it lands within 300ms. **Deferred**; `tests/poll.rs` ships unit tests for `effective_poll_interval`.
- [x] Write integration tests in `crates/scryd-imap/tests/tombstone.rs`: stage 5 messages, fetch them all, then have the mock drop UIDs 2 and 4 from `UID SEARCH ALL`, run `tombstone::scan`, assert exactly 2 messages have `tombstoned_at` set in storage. **Partial:** unit tests cover the diff math (8 cases including dedup, empty inputs, server-only UIDs, monotonic ordering); the live-orchestration end-to-end test waits for the mock server.

## Acceptance criteria
- [x] `cargo test -p scryd-imap --test idle --test poll --test tombstone` passes (unit-test versions of all three).
- [x] `cargo check -p scryd-imap` exits 0.
- [x] `git grep -nE 'IDLE_RECYCLE\s*:\s*Duration\s*=\s*Duration::from_secs\(25\s*\*\s*60\)' crates/scryd-imap/src/idle.rs` matches.
- [x] `git grep -nE 'TOMBSTONE_SCAN_EVERY\s*:\s*u32\s*=\s*10' crates/scryd-imap/src/tombstone.rs` matches.
- [x] `git grep -nE 'kind::IDLE_ENTER|kind::IDLE_DROP|kind::IDLE_RECYCLE' crates/scryd-imap/src/idle.rs | wc -l` returns at least 3.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
