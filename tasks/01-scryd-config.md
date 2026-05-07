Parent slice: [build-and-packaging](../slices/build-and-packaging.md), [observability](../slices/observability.md)
Depends on: 00

# Task 01 — scryd-config

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the typed `config.toml` reader with full validation and a `SecretString` password type that never prints the credential, so every other crate can depend on a single source of truth for accounts and the IMAP password is unprintable by construction.

## Tasks
- [x] In `crates/scryd-config/Cargo.toml`, add deps: `serde` (with `derive`), `serde_json`, `toml`, `secrecy`, `zeroize`, `thiserror`, `anyhow` (workspace-inherited).
- [x] In `crates/scryd-config/src/lib.rs`, define the public types matching the cli slice's config schema: `Config { server: ServerCfg, sync: SyncCfg, indexers: IndexersCfg, accounts: Vec<AccountCfg> }`. The `[server]` section is empty in v1 (XDG paths are derived; document with a doc-comment that the section is reserved).
- [x] Define `SyncCfg { poll_interval_seconds: u32, use_idle: bool, folders: Vec<String> }` with defaults `poll_interval_seconds = 300`, `use_idle = true`, `folders = ["INBOX"]`.
- [x] Define `IndexersCfg { semantic: bool }` with default `semantic = true`.
- [x] Define `AccountCfg { id: String, host: String, port: u16, user: String, password: AccountPassword, folders: Option<Vec<String>> }`.
- [x] Define `AccountPassword(secrecy::SecretString)` as a wrapper newtype in `crates/scryd-config/src/secret.rs`. Implement `serde::Deserialize` so it reads from a `String` field but stores in `SecretString`. Implement `Debug` to print `AccountPassword([REDACTED])`. Do not implement `Display`. Implement a method `expose(&self) -> &str` for callers that need the raw secret (audited at use site).
- [x] In `crates/scryd-config/src/loader.rs`, implement `Config::load(path: &Path) -> Result<Config, ConfigError>` that reads the TOML, validates account_id matches `^[a-z0-9_-]+$` (regex via `regex` crate added to deps), validates `port` in `1..=65535`, validates each account has at least one folder (account override or `[sync] folders` default), and validates account ids are unique. Use `thiserror` for `ConfigError` variants.
- [x] In the same file, implement a `Config::load_from_xdg() -> Result<Config, ConfigError>` helper that resolves `$XDG_CONFIG_HOME` (default `$HOME/.config`) and reads `scryd/config.toml`. Returns a distinct `ConfigError::NotFound` when the file is absent.
- [x] Add `crates/scryd-config/src/permissions.rs` with `pub fn assert_mode_0600(path: &Path) -> Result<(), ConfigError>` that on Unix uses `std::os::unix::fs::MetadataExt::mode()` and verifies the low 9 bits == `0o600`. On non-Unix targets, returns `Ok(())` with a doc-comment explaining v1 is Linux-only. Returns `ConfigError::PermissionTooOpen` otherwise.
- [x] Write unit tests in `crates/scryd-config/tests/loader.rs`: parse a minimal valid config; reject invalid account-id regex; reject port=0 and port=65536; reject duplicate account ids; reject account with empty folders override and empty default; password round-trips through `Debug` as `[REDACTED]`; `expose()` returns the original string.

## Acceptance criteria
- [x] `cargo test -p scryd-config` passes (every unit test above).
- [x] `cargo check -p scryd-config` exits 0.
- [x] `git grep -nE 'impl\s+std::fmt::Display\s+for\s+AccountPassword' crates/scryd-config/` returns no matches (no Display impl).
- [x] `git grep -nE '\[REDACTED\]' crates/scryd-config/` matches the Debug impl.
- [x] `cargo run -p scryd-config --example dump-empty 2>&1 | grep -F '[REDACTED]'` — add a small example at `crates/scryd-config/examples/dump-empty.rs` that constructs an `AccountPassword` with a fake secret and prints it via `{:?}`; the AC asserts the output contains `[REDACTED]` and not the fake secret.
- [x] `test -f crates/scryd-config/src/secret.rs && test -f crates/scryd-config/src/loader.rs && test -f crates/scryd-config/src/permissions.rs`.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
