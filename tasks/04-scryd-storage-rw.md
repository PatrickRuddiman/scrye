Parent slice: [storage](../slices/storage.md)
Depends on: 03, 01

# Task 04 — scryd-storage-rw

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the connection topology (one mutex-guarded write connection + a small read-only pool), the typed query/update helpers, and the account-from-config reconciliation that every other crate consumes.

## Tasks
- [ ] In `crates/scryd-storage/Cargo.toml`, add deps: `tokio` (with `sync` feature), `scryd-config` (path = `../scryd-config`), `uuid` (with `v4` feature).
- [ ] In `crates/scryd-storage/src/handle.rs`, expose `pub struct StorageHandle { writer: Arc<tokio::sync::Mutex<rusqlite::Connection>>, readers: Vec<Arc<tokio::sync::Mutex<rusqlite::Connection>>> }` with `StorageHandle::open(data_dir: &Path, read_pool_size: usize) -> Result<Self, StorageError>` constructing one writer + N readers via `db::open` / `db::open_read_only`.
- [ ] In the same file, expose `pub async fn with_writer<F, R>(&self, f: F) -> Result<R, StorageError>` where `F: FnOnce(&mut Connection) -> Result<R, StorageError>`. Acquire the writer mutex and call `f`. Similarly `with_reader<F, R>` round-robins across the read pool by an internal atomic counter.
- [ ] In `crates/scryd-storage/src/messages.rs`, define a `MessageRow` struct mirroring the `messages` table columns and a `MessageInsert` struct for inserts. Implement `pub async fn insert_message(&self, m: MessageInsert) -> Result<(), StorageError>` using `INSERT … ON CONFLICT(account_id, folder, server_uid, uidvalidity) DO UPDATE SET …` (idempotent per slice §3 Decision 7). Implement `pub async fn get_message(&self, id: &str) -> Result<Option<MessageRow>, StorageError>` and `pub async fn get_thread(&self, thread_id: &str) -> Result<Vec<MessageRow>, StorageError>` filtering `tombstoned_at IS NULL` and ordering by `date_unix ASC`.
- [ ] In `crates/scryd-storage/src/threading.rs`, implement `pub fn resolve_thread_id(conn: &Connection, account_id: &str, header_message_id: Option<&str>, in_reply_to: Option<&str>, references: &[String]) -> Result<String, StorageError>` per slice §3 Decision 8: walk References + In-Reply-To, look up matching `header_message_id` rows for the same account, return the existing `thread_id` if any matches, else mint a new UUID v4 string.
- [ ] In `crates/scryd-storage/src/accounts.rs`, define `AccountRow` and implement `pub async fn list_active_accounts(&self) -> Result<Vec<AccountRow>, StorageError>` (`active = 1` ordered by `account_id`).
- [ ] In `crates/scryd-storage/src/reconcile.rs`, implement `pub async fn reconcile_from_config(&self, cfg: &scryd_config::Config) -> Result<ReconcileDiff, StorageError>` that upserts `accounts` rows from `cfg.accounts`, sets `active=0` on accounts no longer in config (without deleting them), updates `mirrored_at`, and returns `ReconcileDiff { added: Vec<String>, updated: Vec<String>, inactivated: Vec<String> }`. Never writes the password to the DB.
- [ ] In `crates/scryd-storage/src/sync_state.rs`, define `SyncStateRow` and `SyncStateUpdate { last_seen_uid: Option<u32>, account_health: Option<AccountHealth>, last_error: Option<String>, backoff_until: Option<i64>, last_idle_at: Option<i64>, last_full_sync_at: Option<i64>, uidvalidity: Option<u32> }`. Implement `pub async fn get_sync_state(&self, account_id, folder)` and `pub async fn update_sync_state(&self, account_id, folder, update: SyncStateUpdate)` that performs a merge update (only writing the fields that are `Some`).
- [ ] Define `pub enum AccountHealth { Unknown, Active, Degraded, AuthRejected, QuotaExceeded, Unreachable, TlsFailed }` matching the `sync_state.account_health` closed set; implement `Display` and `FromStr` round-tripping the lowercase-with-hyphens form (`"unknown"`, `"auth-rejected"`, etc.).
- [ ] Write integration tests in `crates/scryd-storage/tests/messages.rs`: insert a message, look it up by id, look it up by thread; reinsert with the same `(account_id, folder, server_uid, uidvalidity)` and assert no duplicate row exists; thread resolution finds an existing thread for a child message via `In-Reply-To`. Use `tempfile::TempDir` and `tokio::test`.
- [ ] Write integration tests in `crates/scryd-storage/tests/reconcile.rs`: load a config with two accounts, reconcile, assert two rows; reload with one removed, reconcile, assert one is `active=0`, the other unchanged; assert no password was persisted (column does not exist; `git grep` of the source confirms).

## Acceptance criteria
- [ ] `cargo test -p scryd-storage` passes (all integration tests above + Task 03's tests).
- [ ] `cargo check -p scryd-storage` exits 0.
- [ ] `git grep -nE 'password' crates/scryd-storage/src/migrations/` returns no matches (confirms no `password` column anywhere in storage migrations).
- [ ] `git grep -nE 'INSERT.*ON CONFLICT.*DO UPDATE' crates/scryd-storage/src/messages.rs` matches the idempotent insert.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
