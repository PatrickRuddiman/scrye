Parent plan: scryd v0.3.1 — service pivot
Depends on: none

# Task 06 — wire-witchcraft-indexer

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Replace the placeholder `Arc::new(InMemoryIndexer::new())` in `serve_init` with a real `WitchcraftIndexer` opened against `<data_dir>/witchcraft.sqlite` + `<assets_dir>/`. `InMemoryIndexer` becomes test-only; production paths never construct it. Behaviour-equivalent for the e2e search round-trip (semantic + fulltext + hybrid all dispatch to the witchcraft backend).

## Tasks
- [ ] In `crates/scryd-runtime/src/serve.rs:69` (the line `let indexer: Arc<dyn scryd_search::Indexer> = Arc::new(InMemoryIndexer::new());`), replace with:
  - `let witchcraft_db = scryd_data.join("witchcraft.sqlite");`
  - `let assets = crate::xdg::assets_dir()?;`
  - `let indexer: Arc<dyn scryd_search::Indexer> = Arc::new(scryd_search::WitchcraftIndexer::open(&witchcraft_db, &assets).await.map_err(|e| RuntimeError::PermissionInvariant { path: witchcraft_db.clone(), reason: format!("witchcraft open: {e}") })?);`
- [ ] In `crates/scryd-runtime/src/serve.rs:11-14` (the imports), drop `InMemoryIndexer`; import `WitchcraftIndexer` from `scryd_search`.
- [ ] In `crates/scryd-runtime/src/lib.rs:5-9` (the doc-comment about "the full live orchestration … is partially deferred"), rewrite to describe the current state: serve.rs wires logging, preflight, storage, witchcraft, drainer, scheduler, and the api router into a single async entry point.
- [ ] In `crates/scryd-search/src/lib.rs` re-exports, ensure `WitchcraftIndexer` is `pub use witchcraft_handle::WitchcraftIndexer` (already the case per existing source) and double-check the symbol is reachable.
- [ ] Verify `crates/scryd-search/Cargo.toml`'s `default = ["witchcraft-backend"]` is present so the production binary build pulls the indexer in.
- [ ] Update `crates/scryd-runtime/tests/serve.rs` (the deferred test stub from v0.1.0 task 16): the test should now stub a temp data dir, place a tiny pre-fetched weights file at `<assets>/xtr-weights.gguf`, call `serve_init`, assert the resulting `ServeContext.scheduler` is started AND the witchcraft sqlite file was created. Skip the test if the env var `XTR_ASSETS` (used by `scryd-search`'s own integration tests) is unset — same skip pattern as `crates/scryd-search/tests/witchcraft.rs:27`.

## Acceptance criteria
- [ ] `cargo build --release -p scryd` exits 0 (witchcraft + candle compile cleanly into the release binary).
- [ ] `cargo test -p scryd-runtime --test serve` passes (the new persistence test + any others).
- [ ] `! git grep -F 'InMemoryIndexer::new()' crates/scryd-runtime/src/` (the production path no longer constructs InMemory).
- [ ] `git grep -nE 'WitchcraftIndexer::open' crates/scryd-runtime/src/serve.rs` matches.
- [ ] `! git grep -F 'partially deferred' crates/scryd-runtime/src/lib.rs` (the doc-comment is fresh).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
