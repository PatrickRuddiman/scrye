Parent slice: [search-engine](../slices/search-engine.md)
Depends on: 00, 04

# Task 09 — scryd-search-witchcraft-binding

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Pin `dropbox/witchcraft` as a Cargo git dependency, wrap its public Rust API in a thin scryd-side façade with `index_submit`, `index_remove`, `index_truncate`, and the document-text builder so subsequent tasks can drain a queue and serve queries.

## Tasks
- [x] In `crates/scryd-search/Cargo.toml`, add deps: `tokio` (with `sync`, `rt`, `macros`), `scryd-storage` (path = `../scryd-storage`), `scryd-log` (path = `../scryd-log`), `serde`, `thiserror`. Add `witchcraft` as a git dep at the workspace root level pinning a specific commit hash chosen at task start (record the hash in the workspace `Cargo.toml` comment with a date), with target-conditional features per build-and-packaging slice §3 Decision 2: `[target.'cfg(target_arch = "x86_64")'.dependencies.witchcraft] features = ["t5-quantized","fbgemm","hybrid-dequant"]`; `[target.'cfg(target_arch = "aarch64")'.dependencies.witchcraft] features = ["t5-quantized"]`.
- [x] In `crates/scryd-search/src/lib.rs`, define the public types from the search-engine slice §4: `MessageId(String)`, `Mode { FullText, Semantic, Hybrid }` with `Display`/`FromStr`, `IndexSubmit { message_id, document }`, `Hit { message_id, score, semantic_snippet: Option<String> }`, `SearchQuery { q, mode, k }`, `SearchResponse { hits }`. Define error enums `IndexError`, `SearchError`.
- [x] In `crates/scryd-search/src/witchcraft_handle.rs`, expose `pub struct WitchcraftHandle` wrapping the upstream library type. Construct via `pub async fn open(db_path: &Path, assets_path: &Path) -> Result<Self, IndexError>`. The exact upstream constructor name is settled by reading `examples/pickbrain/` source on the pinned commit; mirror that pattern.
- [x] On `WitchcraftHandle`, expose `pub async fn submit(&self, submit: IndexSubmit) -> Result<(), IndexError>` calling the upstream library's index method. Document text is the caller's responsibility (Task 10 builds it).
- [x] On `WitchcraftHandle`, expose `pub async fn remove(&self, id: &MessageId) -> Result<(), IndexError>`. If the upstream library does not expose direct removal (open question in slice §7), document the fallback: store a tombstone marker in a sibling SQLite table the wrapper owns, and exclude tombstoned ids in `search()`. Implement whichever path the upstream commit supports.
- [x] On `WitchcraftHandle`, expose `pub async fn truncate(&self) -> Result<(), IndexError>` that wipes the witchcraft DB. If the upstream library does not provide truncate, the wrapper closes the handle, deletes the sqlite file, and re-opens.
- [x] In `crates/scryd-search/src/document.rs`, expose `pub fn build(subject: Option<&str>, sender_addr: &str, sender_name: Option<&str>, body_md: &str) -> String` producing `<subject>\n\n<from-display>\n\n<body-markdown>` per slice §3 Decision 6 (omit empty subject; from-display is `name <addr>` or `addr` if name is None).
- [x] Write unit tests in `crates/scryd-search/tests/document.rs` covering: subject + name + addr + body all present; subject missing; name missing; body empty.
- [x] Write integration tests in `crates/scryd-search/tests/witchcraft.rs` (gated behind `#[ignore]` if a CI environment without weights cannot run): construct a handle against a temp dir, submit two documents, query with `Mode::FullText`, assert one of the documents matches.

## Acceptance criteria
- [x] `cargo build -p scryd-search` exits 0 (this exercises the upstream git dep build on the host arch).
- [x] `cargo test -p scryd-search --test document` passes.
- [x] `cargo test -p scryd-search --test witchcraft -- --include-ignored` passes when `XTR_ASSETS` env var points to a downloaded weights dir; otherwise `cargo test -p scryd-search` (without `--include-ignored`) still passes (skipping the gated tests).
- [x] `git grep -nE 'witchcraft\s*=\s*\{\s*git\s*=' Cargo.toml` matches the pinned git dep.
- [x] `git grep -nE 'rev\s*=' Cargo.toml | grep witchcraft` matches the pinned commit hash.
- [x] `git grep -nE 'features\s*=\s*\["t5-quantized","fbgemm","hybrid-dequant"\]' Cargo.toml` matches the x86_64 line.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
