Parent slice: [imap-sync](../slices/imap-sync.md)
Depends on: 12, 04, 05, 07

# Task 13 — scryd-imap-fetch

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the per-(account, folder) connection state machine, UID-keyed FETCH with `BODY.PEEK[]` in 500-UID chunks, UIDVALIDITY change handling, and the wiring that hands each fetched message to mime parsing and storage.

## Tasks
- [ ] In `crates/scryd-imap/src/state.rs`, define `pub enum ConnState { Disconnected, Resolving, Connecting, TlsHandshaking, LoggingIn, CapabilityChecking, Selecting, InitialBackfilling { last_uid: u32, target: Option<u32> }, Idling, Polling, Fetching, Backoff { until: Instant } }` and `pub struct Connection { account_id, folder, state, … }`. Define `INCREMENTAL_FETCH_BATCH = 500`.
- [ ] In `crates/scryd-imap/src/fetch.rs`, expose `pub async fn fetch_batch(client: &mut Client, range: (u32, u32)) -> Result<Vec<FetchedMessage>, ClientError>` issuing `UID FETCH <lo>:<hi> (UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])` and returning a `FetchedMessage { account_id, folder, server_uid, uidvalidity, internal_date, flags: Vec<String>, raw_bytes: Vec<u8> }` per element. (`account_id`/`folder`/`uidvalidity` are filled by the caller.)
- [ ] Define `MessageSink` trait in `crates/scryd-imap/src/sink.rs` per slice §4: `submit`, `tombstone`, `update_sync_state`. Provide an impl in `crates/scryd-runtime/` later (Task 16); for the integration test here, provide a test-only impl backed by `scryd-storage` + `scryd-mime`.
- [ ] Implement `pub async fn run_initial_backfill(conn: &mut Connection, client: &mut Client, sink: &dyn MessageSink) -> Result<(), ClientError>`: EXAMINE the folder → read UIDVALIDITY → if storage has a different stored uidvalidity, set conn.state to `InitialBackfilling { last_uid: 0, target: server_uidnext - 1 }`; iterate `1:500`, `501:1000`, …, calling `fetch_batch` and `sink.submit` for each result; after each batch, `sink.update_sync_state(SyncStateUpdate { last_seen_uid: Some(hi), uidvalidity: Some(server_uidvalidity), … })`. On `target` reached, return.
- [ ] Implement `pub async fn run_incremental(conn: &mut Connection, client: &mut Client, sink: &dyn MessageSink) -> Result<(), ClientError>`: EXAMINE → check UIDVALIDITY (route to backfill if changed) → `UID FETCH <last_seen_uid+1>:* (…)` → process each → update watermark.
- [ ] In `crates/scryd-imap/src/uidvalidity.rs`, expose `pub async fn handle_change(conn: &mut Connection, sink: &dyn MessageSink, new_uidvalidity: u32) -> Result<(), ClientError>` that: (a) emits a `log_lifecycle!(kind = kind::UIDVALIDITY_RESET, …)` event, (b) clears `last_seen_uid` for the (account, folder) via `sink.update_sync_state`, (c) resets `conn.state` to `InitialBackfilling { last_uid: 0, target: None }`. The slice's category for the spec's failure list is `UID_VALIDITY_RESET` but it's emitted at INFO severity per observability slice §3 Decision 6.
- [ ] Wire mime parsing into the test-only `MessageSink` impl: each `FetchedMessage.raw_bytes` is passed to `scryd_mime::parse(...)`; on `Parsed`/`ParsedDegraded`, write a `messages` row + `attachments` rows + raw .eml file via the storage helpers; on `Unparseable`, write a placeholder row + raw .eml only.
- [ ] Write integration tests in `crates/scryd-imap/tests/fetch.rs` against the mock server from Task 12: stage 7 messages on the mock; run `run_initial_backfill`; assert all 7 land in storage with correct `uidvalidity`/`server_uid`; assert raw .eml files exist on disk; restart the connection mid-batch (simulate by killing after 500-row commit) and run again — assert no duplicate messages.
- [ ] Write integration tests for UIDVALIDITY change: stage messages with uidvalidity 100, fetch them, then have the mock report uidvalidity 200; run `run_incremental`; assert the existing rows remain (under uidvalidity 100) and new rows land under uidvalidity 200.

## Acceptance criteria
- [ ] `cargo test -p scryd-imap --test fetch` passes.
- [ ] `cargo check -p scryd-imap` exits 0.
- [ ] `git grep -nE 'INCREMENTAL_FETCH_BATCH\s*:\s*u32\s*=\s*500' crates/scryd-imap/src/state.rs` matches.
- [ ] `git grep -nE 'BODY\.PEEK\[\]' crates/scryd-imap/src/fetch.rs` matches the fetch attribute list.
- [ ] `git grep -nE 'EXAMINE' crates/scryd-imap/src/` matches at least one usage; `git grep -nE 'SELECT[^/]' crates/scryd-imap/src/` returns no matches that look like an IMAP `SELECT` (other than SQL `SELECT` if any test imports happen to contain that string).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
