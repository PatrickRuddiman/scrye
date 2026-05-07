Parent slice: [storage](../slices/storage.md)
Depends on: 03

# Task 05 — scryd-storage-raw

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the raw `.eml` file store: `raw/<account_id>/<yyyy>/<mm>/<sha256-hex-of-MessageId>.eml` layout, atomic `.tmp` + rename writes, mode `0600` files, and a streaming reader for the api slice.

## Tasks
- [x] In `crates/scryd-storage/Cargo.toml`, add deps: `sha2`, `hex`, `chrono` (with `clock` feature, for unix-seconds → year/month).
- [x] In `crates/scryd-storage/src/raw.rs`, implement `pub fn raw_path(data_dir: &Path, account_id: &str, message_id: &str, date_unix: i64) -> PathBuf` producing `<data_dir>/raw/<account_id>/<YYYY>/<MM>/<sha256-hex(message_id)>.eml`. SHA-256 the message_id bytes; lowercase hex.
- [x] In the same file, implement `pub async fn write_raw(data_dir: &Path, account_id: &str, message_id: &str, date_unix: i64, bytes: &[u8]) -> Result<PathBuf, StorageError>`. Create parent dirs (mode `0700`), write to `<final-path>.tmp` mode `0600`, `fsync`, `rename(2)` to `<final-path>`. On Linux, set mode via `std::os::unix::fs::OpenOptionsExt::mode(0o600)`. Return the resolved path.
- [x] In the same file, implement `pub async fn open_raw(path: &Path) -> Result<tokio::fs::File, StorageError>` returning a tokio file handle suitable for `ReaderStream::new` consumption by the api slice's raw-bytes endpoint.
- [x] Implement `pub async fn delete_raw(path: &Path) -> Result<(), StorageError>` for the (out-of-scope-in-v1 but type-present) future cleanup. Mark with a `#[doc(hidden)]` until v2.
- [x] Write integration tests in `crates/scryd-storage/tests/raw.rs`: write 1024 bytes for `(account="primary", message_id="primary:abc@x", date=2026-04-15 epoch)`, assert the path is `<tmp>/raw/primary/2026/04/<sha256>.eml`, file mode is `0600`, contents match. Open with `open_raw`, read all bytes, assert match. Concurrently call `write_raw` twice for the same id; assert the final file is one of the two writes (atomic).
- [x] Write a unit test asserting `raw_path` is deterministic — same inputs → same output across calls.

## Acceptance criteria
- [x] `cargo test -p scryd-storage --test raw` passes.
- [x] `cargo check -p scryd-storage` exits 0.
- [x] `git grep -nE 'mode\(0o600\)' crates/scryd-storage/src/raw.rs` matches.
- [x] `git grep -nE 'sha2::Sha256|sha256' crates/scryd-storage/src/raw.rs` matches.
- [x] `git grep -nE 'rename\(' crates/scryd-storage/src/raw.rs` matches the atomic-rename call.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
