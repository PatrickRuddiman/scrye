Parent plan: scryd v0.3.1 — service pivot
Depends on: none

# Task 00 — server-config-table

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Add a `[server]` table to `Config` carrying the two knobs the rest of the cycle reads (`require_peer_uid`, `socket_mode`), plus an optional `tls_ca_path` field per `[[accounts]]` entry. All defaults preserve current behaviour for existing configs that don't mention the new fields.

## Tasks
- [ ] In `crates/scryd-config/src/loader.rs:24-25` (after the existing `ServerCfg` empty-struct stub), define the populated form:
  - `pub struct ServerCfg { #[serde(default = "default_require_peer_uid")] pub require_peer_uid: bool, #[serde(default = "default_socket_mode")] pub socket_mode: u32 }`.
  - `fn default_require_peer_uid() -> bool { false }`.
  - `fn default_socket_mode() -> u32 { 0o666 }`.
  - Implement `Default for ServerCfg` returning the same default values.
- [ ] In `crates/scryd-config/src/loader.rs:79-89` (`AccountCfg`), add `#[serde(default)] pub tls_ca_path: Option<std::path::PathBuf>`.
- [ ] Re-export `ServerCfg` from `crates/scryd-config/src/lib.rs` alongside the other types.
- [ ] Document each new field with a one-line `///` doc-comment naming the default and what it controls.
- [ ] Extend `crates/scryd-config/tests/loader.rs` with three tests: `server_table_defaults_when_absent` (empty config → ServerCfg::default), `server_table_explicit_values_round_trip` (TOML with `[server]` block sets fields), `account_tls_ca_path_round_trips` (TOML with `tls_ca_path = "/etc/foo.pem"` parses to `Some(PathBuf)`).

## Acceptance criteria
- [ ] `cargo test -p scryd-config --test loader` passes (existing tests + the three new ones).
- [ ] `cargo build --workspace` exits 0.
- [ ] `git grep -nE 'pub struct ServerCfg' crates/scryd-config/src/loader.rs` matches exactly once.
- [ ] `git grep -nE 'pub require_peer_uid|pub socket_mode|pub tls_ca_path' crates/scryd-config/src/loader.rs | wc -l` returns at least 3.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
