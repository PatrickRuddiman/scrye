Parent plan: GreenMail integration testing for scryd
Parent v0.1.0 task: [13-scryd-imap-fetch.md](../13-scryd-imap-fetch.md)
Depends on: 00, 01

# Task 02 — imap-fetch-parsing

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the live response-parsing for `fetch_batch` in `crates/scryd-imap/src/fetch.rs:23-46`. After this task, `fetch_batch` returns a `Vec<FetchedMessage>` populated from a real `UID FETCH … (UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])` response. Higher loops (initial-backfill, incremental, IDLE, poll) come in tasks 03-06.

## Tasks
- [x] In `crates/scryd-imap/src/fetch.rs`, replace the deferred `Err("…")` return body with the real implementation:
  - Issue `client.uid_fetch(format!("{lo}:{hi}"), FETCH_ATTRS)`.
  - For each `Fetch` element returned, extract `uid()`, `internal_date()`, `flags()`, and the `BODY[]` byte slice. Map missing fields to `ClientError::Server` with the missing-attr name.
  - Build a `FetchedMessage { account_id: String::new(), folder: String::new(), server_uid, uidvalidity: 0, internal_date, flags, raw_bytes }` per row. (`account_id` / `folder` / `uidvalidity` are filled by the caller — keep that contract.)
  - Return `Ok(Vec<FetchedMessage>)`.
- [x] Add an integration test `crates/scryd-imap/tests/fetch_live.rs`:
  - `setup`: skip-if-unreachable; `clear_mailbox()`; `inject_n(3, "test@localhost")`.
  - Connect via `connect::login(.., tls = false, .., user = "test", password = "test", ..)`.
  - `client.examine("INBOX")`.
  - Call `fetch_batch(&mut client, (1, 100))`.
  - Assert exactly 3 messages returned, each with non-empty `raw_bytes` containing `Subject: test-`, distinct `server_uid` values, and `internal_date` set.
- [x] Add a unit test for the missing-attr path: feed a hand-rolled `Fetch` with no UID and assert `ClientError::Server`. (Trickier; if `async-imap` doesn't expose a constructor for `Fetch` outside its own crate, mock at the function boundary by extracting the parsing into a helper `parse_fetch_row(fetch: &Fetch) -> Result<FetchedMessage, ClientError>` that's testable with a stub.)

## Acceptance criteria
- [x] `cargo test -p scryd-imap --test fetch_live` passes locally with GreenMail running.
- [x] `cargo test -p scryd-imap` overall passes (existing tests still green).
- [x] `! git grep -F 'live response parsing is authored in a follow-up' crates/scryd-imap/src/fetch.rs` (the deferred-comment is gone).
- [x] `git grep -nE 'pub async fn fetch_batch' crates/scryd-imap/src/fetch.rs` matches and the body no longer returns `Err`.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
