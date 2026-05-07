Parent slice: [build-and-packaging](../slices/build-and-packaging.md)
Depends on: none

# Task 00 — workspace-scaffold

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Stand up the Cargo workspace, all empty member-crate stubs (one per slice), the release profile, the `.gitignore`, and a placeholder binary, so every subsequent task has a concrete crate path to land code in.

## Tasks
- [x] Create `Cargo.toml` at the repo root as a workspace manifest. Set `[workspace]` `resolver = "2"`, `members = ["crates/scryd-config", "crates/scryd-log", "crates/scryd-storage", "crates/scryd-mime", "crates/scryd-search", "crates/scryd-imap", "crates/scryd-api", "crates/scryd-runtime", "scryd"]`. Set `[workspace.package]` with `edition = "2021"`, `rust-version = "1.78"`, `license = "Apache-2.0"`, `version = "0.1.0"`.
- [x] In the same root `Cargo.toml`, declare `[workspace.dependencies]` with placeholders (no version yet, just names so member crates can do `dep = { workspace = true }` later) for: `tokio`, `axum`, `rustls`, `tokio-rustls`, `webpki-roots`, `rusqlite`, `serde`, `serde_json`, `clap`, `rpassword`, `toml_edit`, `reqwest`, `mail-parser`, `htmd`, `async-imap`, `tracing`, `tracing-subscriber`, `secrecy`, `zeroize`, `thiserror`, `anyhow`. Each entry minimally gives a fixed version; subsequent tasks bump features as needed.
- [x] Add `[profile.release]` to root `Cargo.toml`: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`.
- [x] For each member crate listed above except `scryd`, create `crates/<crate>/Cargo.toml` and `crates/<crate>/src/lib.rs`. The `Cargo.toml` inherits `[package]` fields from the workspace (`edition.workspace = true` etc.) and sets `name` to the crate name. The `lib.rs` is empty (zero bytes acceptable, or a doc comment).
- [x] Create `scryd/Cargo.toml` declaring `[package]` with workspace inheritance and `[[bin]] name = "scryd"`. Create `scryd/src/main.rs` with a `fn main() {}` placeholder.
- [x] Create `.gitignore` at the repo root with `target/`, `Cargo.lock` is **not** ignored (binary crate), `*.swp`, `*.tmp`, `.DS_Store`.
- [x] Create a top-level empty `ops/` directory with a `.gitkeep` so subsequent build-and-packaging tasks have a destination.
- [x] Create `rust-toolchain.toml` at the repo root pinning `channel = "1.78"` so contributors and CI agree.

## Acceptance criteria
- [x] `cargo check --workspace` exits 0.
- [x] `cargo build --release -p scryd` exits 0 and produces `target/release/scryd` (or `scryd.exe` on Windows dev hosts; CI is Linux only).
- [x] `cargo test --workspace` exits 0 (no tests yet, but the harness must compile and report `0 passed`).
- [x] `test -f Cargo.toml && test -f scryd/Cargo.toml && test -f scryd/src/main.rs && test -f .gitignore && test -f rust-toolchain.toml`.
- [x] `for c in scryd-config scryd-log scryd-storage scryd-mime scryd-search scryd-imap scryd-api scryd-runtime; do test -f crates/$c/Cargo.toml && test -f crates/$c/src/lib.rs || exit 1; done`.
- [x] `git grep -nE 'opt-level\s*=\s*3' Cargo.toml` matches at least one line.
- [x] `git grep -nE 'panic\s*=\s*"abort"' Cargo.toml` matches at least one line.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
