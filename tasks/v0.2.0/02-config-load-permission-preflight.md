Parent slice: [scryd v0.2.0 — security](../../slices/0.2.0/security.md)
Depends on: none

# Task 02 — config-load-permission-preflight

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Wire the existing-but-unused `assert_mode_0600()` into `Config::load` so the daemon refuses to start when the config file is world-readable, surface the failure as a typed error, and prove both the refuse-and-accept paths in tests.

## Tasks
- [ ] In `crates/scryd-config/src/loader.rs:115-126` (`Config::load`), call `crate::permissions::assert_mode_0600(path)` before the TOML deserialization step. On failure, propagate as a new variant of the loader's error enum.
- [ ] In `crates/scryd-config/src/loader.rs` error type, add `ConfigError::PermissionInvariant { path: PathBuf, mode_seen: u32 }` variant (or extend the existing variant if one is named similarly). Map any `Err` returned from `assert_mode_0600` into this variant so the daemon-startup path emits one error naming the path and the seen mode.
- [ ] In `crates/scryd-config/src/permissions.rs:10-20`, refactor `assert_mode_0600` if needed so it returns `Result<(), (PathBuf, u32)>` (path + mode_seen) the loader can map cleanly. Keep the function `pub(crate)` if no other crate reads it.
- [ ] Add tests to `crates/scryd-config/tests/loader.rs`: `load_refuses_world_readable_config` writes a TOML at mode `0o644`, calls `Config::load`, asserts `Err(ConfigError::PermissionInvariant { mode_seen: 0o644, .. })`. `load_accepts_mode_0600_config` writes the same TOML at mode `0o600`, calls `Config::load`, asserts `Ok`. Both use `tempfile::TempDir`.
- [ ] In `crates/scryd-config/tests/loader.rs`, ensure the new tests run after any existing `load_*` test that creates a config at the default umask — the umask under `cargo test` may differ from `0o077`; the new tests must `chmod` explicitly with `std::fs::set_permissions` before calling `Config::load`.

## Acceptance criteria
- [ ] `cargo test -p scryd-config --test loader` passes (existing tests + the two new ones, all green).
- [ ] `git grep -n 'assert_mode_0600' crates/scryd-config/src/loader.rs` matches the new call site (the function was previously uncalled in `src/`).
- [ ] `git grep -nE 'PermissionInvariant' crates/scryd-config/src/loader.rs` matches the error mapping.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
