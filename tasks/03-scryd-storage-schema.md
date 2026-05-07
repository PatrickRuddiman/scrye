Parent slice: [storage](../slices/storage.md)
Depends on: 00

# Task 03 — scryd-storage-schema

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the `meta.sqlite` schema, all indexes, the migration runner, and WAL-mode opening, so every storage-consuming task can rely on a fully-shaped database.

## Tasks
- [x] In `crates/scryd-storage/Cargo.toml`, add deps: `rusqlite` (with `bundled` feature so SQLite is statically linked), `serde`, `serde_json`, `thiserror`, `anyhow`, `tracing`, `scryd-log` (path = `../scryd-log`).
- [x] Create `crates/scryd-storage/src/migrations/` directory. Add `mod.rs` exposing `pub fn run(conn: &mut rusqlite::Connection) -> Result<(), MigrationError>`. The runner reads `schema_version.version` (creating the table if absent), iterates over an inline `&[(u32, &str)]` ordered list of `(version, sql)` migrations, applies each one whose version > current inside a transaction, and bumps `schema_version.version` after each applied migration.
- [x] In `crates/scryd-storage/src/migrations/v1_initial.rs`, define the `pub const SQL: &str` for migration 1 creating: `schema_version`, `accounts`, `sync_state`, `messages`, `attachments`, `index_queue`. Column lists must match the `§4 Contracts & shapes` block of `slices/storage.md` exactly.
- [x] In the same migration, add the secondary indexes the storage slice §4 enumerates: `(date_unix DESC)`, `(thread_id, date_unix ASC)`, `(sender_addr, date_unix DESC)`, `(folder, date_unix DESC)`, `(account_id, date_unix DESC)` on `messages`; `(message_id)` on `attachments`; `(failed_permanent, queued_at ASC)` on `index_queue`.
- [x] In the same migration, declare `UNIQUE(account_id, folder, server_uid, uidvalidity)` on `messages` and the `FOREIGN KEY` constraints listed in the slice.
- [x] In `crates/scryd-storage/src/db.rs`, expose `pub fn open(path: &Path) -> Result<rusqlite::Connection, StorageError>` that opens with `rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | SQLITE_OPEN_CREATE`, sets `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`, `PRAGMA foreign_keys=ON`, and runs `migrations::run`. Return `StorageError::Permission` (mapping to category `configuration permission error`) when the OS rejects the open.
- [x] In `crates/scryd-storage/src/db.rs`, also expose `pub fn open_read_only(path: &Path) -> Result<rusqlite::Connection, StorageError>` opening with `SQLITE_OPEN_READ_ONLY` and the same pragmas (only those that work read-only). For use by the read pool (Task 04).
- [x] Write integration tests in `crates/scryd-storage/tests/migrations.rs` using a `tempfile::TempDir`: open a fresh DB, assert `schema_version.version` is the highest registered version, assert all expected tables/indexes exist via `sqlite_master`, re-open and assert no migrations re-run.
- [x] Add a separate `crates/scryd-storage/tests/wal.rs` integration test that opens a fresh DB and asserts `journal_mode` returns `wal`.

## Acceptance criteria
- [x] `cargo test -p scryd-storage` passes (migrations + wal tests).
- [x] `cargo check -p scryd-storage` exits 0.
- [x] `cargo run -p scryd-storage --example dump-schema 2>&1 | grep -E '^CREATE TABLE messages'` succeeds — add `examples/dump-schema.rs` that opens an in-memory or temp DB, runs migrations, and prints `sqlite_master.sql` for each table.
- [x] `git grep -nE 'UNIQUE\s*\(\s*account_id\s*,\s*folder\s*,\s*server_uid\s*,\s*uidvalidity\s*\)' crates/scryd-storage/` matches the migration SQL.
- [x] `git grep -nE 'CREATE INDEX' crates/scryd-storage/src/migrations/` returns at least 7 matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
