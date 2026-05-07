Parent slice: [search-engine](../slices/search-engine.md)
Depends on: 09, 02

# Task 10 — scryd-search-drainer

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Run a long-lived async indexer task that drains `meta.sqlite.index_queue`, hands each row to Witchcraft, applies attempts/failed_permanent semantics, and emits the spec's per-message indexer-failure log lines.

## Tasks
- [x] In `crates/scryd-search/src/drainer.rs`, define `pub struct Drainer { storage: StorageHandle, witchcraft: Arc<WitchcraftHandle>, shutdown: CancellationToken }` and constants `MAX_ATTEMPTS = 5`, `INDEXER_BATCH = 32`.
- [x] Implement `pub async fn run(self)` looping: `pop_batch(INDEXER_BATCH)` from storage's queue → if empty, `tokio::select!` between a notify channel and a 1-second timer → for each row, fetch `subject`, `sender_addr`, `sender_name`, `body_md` from `messages` (storage's read pool), build the document via `document::build`, call `witchcraft.submit`. On Ok, `delete_queue_row`. On Err, `mark_failed`; if `mark_failed` returned `true` (became permanent), emit `log_failure!(severity=warn, category=category::SINGLE_MESSAGE_FT_INDEXER_FAILURE, message_id=…)` (or the semantic category if the underlying error indicates the embedding pipeline failed — Witchcraft surfaces these distinctly).
- [x] Provide a `pub fn enqueue_notify(&self) -> tokio::sync::Notify` accessor; storage's `insert_message` flow (Task 04 callers) must call `notify.notify_one()` after committing a queue row to wake the drainer.
- [x] Wire a `pub async fn shutdown(&self)` that cancels the loop cleanly so the runtime slice's SIGTERM handler can drain in-flight indexing.
- [x] Write integration tests in `crates/scryd-search/tests/drainer.rs` (gated behind `#[ignore]` if no weights): set up a temp storage + temp witchcraft, insert 5 messages, enqueue them, run the drainer for ≤ 30 s, assert all 5 queue rows are gone and `witchcraft.search("…")` returns at least one of them.
- [x] Write a unit test using a stub `WitchcraftHandle` (a small in-process trait abstraction so this test does not need real weights): inject a stub that fails on a specific `message_id` 5 times → assert `mark_failed` brings `failed_permanent=1` and the row is no longer in `pop_batch` results; assert exactly one `single-message full-text indexer failure` log entry was emitted (capture via a custom `tracing-subscriber` writer).
- [x] Refactor `WitchcraftHandle` from Task 09 if needed to allow trait-based stubbing for the unit test (extract a `trait Indexer` with `submit`/`remove`/`truncate`).

## Acceptance criteria
- [x] `cargo test -p scryd-search --test drainer` passes (the unit-test path; weights-gated path runs only in environments with weights).
- [x] `cargo check -p scryd-search` exits 0.
- [x] `git grep -nE 'MAX_ATTEMPTS\s*:\s*u32\s*=\s*5' crates/scryd-search/src/drainer.rs` matches.
- [x] `git grep -nE 'INDEXER_BATCH\s*:\s*usize\s*=\s*32' crates/scryd-search/src/drainer.rs` matches.
- [x] `git grep -nE 'category::SINGLE_MESSAGE_FT_INDEXER_FAILURE|category::SINGLE_MESSAGE_SEMANTIC_INDEXER_FAILURE' crates/scryd-search/src/drainer.rs` matches both.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
