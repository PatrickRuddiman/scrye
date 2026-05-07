Parent slice: [storage](../slices/storage.md)
Depends on: 04

# Task 06 — scryd-storage-tombstones-reindex

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the tombstone marker (`messages.tombstoned_at`), the queue helpers (`enqueue`, `pop_batch`, `mark_failed`), and `reenqueue_all_messages()` so the api slice's reindex flow has a one-call repopulate.

## Tasks
- [x] In `crates/scryd-storage/src/queue.rs`, define `IndexQueueRow { message_id, attempts, last_error: Option<String>, queued_at, failed_permanent }`. Implement `pub async fn enqueue(&self, message_id: &str) -> Result<(), StorageError>` using `INSERT OR IGNORE INTO index_queue(...) VALUES (?, 0, NULL, ?, 0)`.
- [x] Implement `pub async fn pop_batch(&self, limit: usize) -> Result<Vec<IndexQueueRow>, StorageError>` selecting `SELECT … FROM index_queue WHERE failed_permanent = 0 ORDER BY queued_at ASC LIMIT ?`.
- [x] Implement `pub async fn delete_queue_row(&self, message_id: &str) -> Result<(), StorageError>` for the success path.
- [x] Implement `pub async fn mark_failed(&self, message_id: &str, err: &str, max_attempts: u32) -> Result<bool, StorageError>` that increments `attempts`, records `last_error`, and sets `failed_permanent = 1` when `attempts + 1 >= max_attempts`. Returns `true` if it became permanent.
- [x] In `crates/scryd-storage/src/messages.rs`, implement `pub async fn tombstone(&self, message_id: &str) -> Result<(), StorageError>` setting `tombstoned_at = strftime('%s','now')` for the row. Idempotent.
- [x] In `crates/scryd-storage/src/queue.rs`, implement `pub async fn reenqueue_all_messages(&self) -> Result<u64, StorageError>` using a single statement `INSERT OR REPLACE INTO index_queue(message_id, attempts, last_error, queued_at, failed_permanent) SELECT message_id, 0, NULL, strftime('%s','now'), 0 FROM messages WHERE tombstoned_at IS NULL`. Return the affected row count.
- [x] Write integration tests in `crates/scryd-storage/tests/queue.rs`: enqueue three message ids, pop a batch of two (assert order by `queued_at`), delete one, mark another failed (max_attempts=3) twice (assert `failed_permanent=0`), once more (assert returns `true`, `failed_permanent=1`), `pop_batch` no longer returns it.
- [x] Write integration tests in `crates/scryd-storage/tests/tombstone.rs`: insert a message, tombstone it, `get_message` returns the row but `tombstoned_at` is set; `get_thread` excludes it.
- [x] Write integration tests for `reenqueue_all_messages`: insert 5 messages (1 tombstoned), call `reenqueue_all_messages`, assert returns 4, `index_queue` has 4 rows with `attempts=0` and `failed_permanent=0`.

## Acceptance criteria
- [x] `cargo test -p scryd-storage --test queue --test tombstone` passes.
- [x] `cargo check -p scryd-storage` exits 0.
- [x] `git grep -nE 'INSERT OR REPLACE INTO index_queue' crates/scryd-storage/src/queue.rs` matches.
- [x] `git grep -nE 'tombstoned_at IS NULL' crates/scryd-storage/src/` matches at least 3 lines (messages, threading, reenqueue).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
