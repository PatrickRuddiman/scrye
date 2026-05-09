Parent plan: GreenMail integration testing for scryd
Parent v0.1.0 task: [14-scryd-imap-idle-poll-tombstone.md](../14-scryd-imap-idle-poll-tombstone.md)
Depends on: 03

# Task 06 — imap-tombstone-scan

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `tombstone::scan` orchestration in `crates/scryd-imap/src/tombstone.rs`. The pure `compute_tombstones` diff is already done; this task wraps it with the live `UID SEARCH ALL` call against the client and the storage query for the local UID set.

## Tasks
- [x] In `crates/scryd-imap/src/tombstone.rs`, add `pub async fn scan(conn: &Connection, client: &mut Client<S>, sink: &dyn MessageSink) -> Result<u32, ClientError>`. Order: `client.uid_search("ALL")` → query storage for `(account_id, folder, uidvalidity)` → `compute_tombstones(local_uids, server_uids)` → for each tombstoned uid, `sink.tombstone(message_id)`. Return the count.
- [x] Generic over `S`.
- [x] Add `crates/scryd-imap/tests/tombstone_live.rs`:
  - skip-if-unreachable; clear_mailbox; inject 5 messages.
  - run_initial_backfill → assert 5 messages stored.
  - Delete 2 messages via GreenMail's REST API (or by EXPUNGE-via-greenmail-admin — check the support module's `delete_uids(user, [uid_list])` helper; add it to the support module if missing).
  - Run `tombstone::scan`. Assert returns 2; assert exactly 2 storage rows have `tombstoned_at` set.

## Acceptance criteria
- [x] `cargo test -p scryd-imap --test tombstone_live` passes locally with GreenMail.
- [x] `git grep -nE 'pub async fn scan' crates/scryd-imap/src/tombstone.rs` matches.
- [x] Existing `cargo test -p scryd-imap --test tombstone` still passes (the `compute_tombstones` unit tests).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
