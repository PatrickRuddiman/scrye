Parent plan: GreenMail integration testing for scryd
Parent v0.1.0 task: [13-scryd-imap-fetch.md](../13-scryd-imap-fetch.md)
Depends on: 02

# Task 03 — imap-initial-and-incremental

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `run_initial_backfill` and `run_incremental` in `crates/scryd-imap/src/fetch.rs` per task 13's specification, plus a test-only `MessageSink` impl backed by `scryd-storage` + `scryd-mime` so the integration test can drive end-to-end (fetch → parse → write). UIDVALIDITY change handling routes via `crate::uidvalidity::handle_change` (already implemented).

## Tasks
- [x] In `crates/scryd-imap/src/fetch.rs`, add `pub async fn run_initial_backfill(conn: &mut Connection, client: &mut Client<S>, sink: &dyn MessageSink) -> Result<(), ClientError>`. Order: EXAMINE → read UIDVALIDITY → if storage uidvalidity differs, route via `uidvalidity::handle_change`; iterate `1:500`, `501:1000`, …, calling `fetch_batch` and `sink.submit` per row; after each batch, `sink.update_sync_state(SyncStateUpdate { last_seen_uid: Some(hi), uidvalidity: Some(server_uidvalidity), .. })`. On `target` (server_uidnext - 1) reached, return.
- [x] Add `pub async fn run_incremental(conn: &mut Connection, client: &mut Client<S>, sink: &dyn MessageSink) -> Result<(), ClientError>`: EXAMINE → uidvalidity check → `UID FETCH <last_seen_uid+1>:* (FETCH_ATTRS)` → process each → update watermark.
- [x] Both helpers must be generic over `Client<S>` (S: AsyncRead + AsyncWrite + Unpin + Send + Debug) so the same code paths work over plain or TLS streams.
- [x] Add `crates/scryd-imap/tests/support/test_sink.rs`: a `TestSink { storage: StorageHandle, mime: ... }` impl of `MessageSink` for tests. `submit` calls `scryd_mime::parse(raw_bytes, ctx)` → on `Parsed`/`ParsedDegraded` writes message row + raw .eml + attachments + enqueue; on `Unparseable` writes placeholder row.
- [x] Add `crates/scryd-imap/tests/initial_backfill_live.rs`:
  - skip-if-unreachable; clear_mailbox; inject_n(7, "test@localhost").
  - login plain → examine INBOX → run_initial_backfill against TestSink backed by a `tempfile::TempDir` storage.
  - Assert 7 messages in `messages` table, each with the right `server_uid` and `uidvalidity`.
  - Assert 7 raw .eml files on disk under the temp data dir.
- [x] Add `crates/scryd-imap/tests/incremental_live.rs`:
  - Run `run_initial_backfill` for 5 messages → close → inject 3 more via SMTP → run `run_incremental` → assert 8 total messages, no duplicates by `server_uid`.

## Acceptance criteria
- [x] `cargo test -p scryd-imap --test initial_backfill_live --test incremental_live` passes locally with GreenMail.
- [x] `git grep -nE 'pub async fn run_initial_backfill' crates/scryd-imap/src/fetch.rs` matches.
- [x] `git grep -nE 'pub async fn run_incremental' crates/scryd-imap/src/fetch.rs` matches.
- [x] `cargo test -p scryd-imap` overall passes.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
