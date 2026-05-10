Parent plan: scryd v0.3.1 — service pivot
Depends on: 06

# Task 07 — witchcraft-persistent-db

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Verify (and lock in) that `WitchcraftIndexer` persists across daemon restarts. The wiring from task 06 already hands it a path; this task is the persistence-survives-restart integration test that catches future regressions where someone accidentally re-points the indexer at a tempdir.

## Tasks
- [x] Add `crates/scryd-runtime/tests/serve.rs::witchcraft_index_survives_restart` (skipped via the same `XTR_ASSETS`-env-var gate used elsewhere). The test:
  - Constructs a temp `data_dir`, places a tiny pre-fetched `xtr-weights.gguf` at `<assets>/`, configures one stub account.
  - Calls `serve_init` to get `ServeContext`; submits 5 fixture documents directly through the indexer (no IMAP); confirms `Searcher::search` returns >=5 hits.
  - Calls `serve_run` shutdown path (or directly drops the ServeContext).
  - Constructs a fresh `serve_init` against the SAME temp `data_dir` (i.e., re-opens the existing `witchcraft.sqlite`).
  - Confirms `Searcher::search` STILL returns the 5 hits without any re-submit. The witchcraft.sqlite file must be the source of truth.
- [x] Confirm `crates/scryd-runtime/src/serve.rs` does NOT pass any "create-fresh-on-startup" flag to `WitchcraftIndexer::open`; the existing API opens-or-creates which is what we want.
- [x] Add a one-line note to `crates/scryd-runtime/src/serve.rs` (just before the `WitchcraftIndexer::open` call) documenting that the sqlite path is the persistence boundary; deleting the file resets the index.

## Acceptance criteria
- [x] `cargo test -p scryd-runtime --test serve witchcraft_index_survives_restart` passes when `XTR_ASSETS` is set; test is skipped silently (with eprintln hint) otherwise.
- [x] `git grep -nE 'witchcraft\.sqlite' crates/scryd-runtime/src/serve.rs` matches.
- [x] `git grep -nE 'witchcraft_index_survives_restart' crates/scryd-runtime/tests/serve.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
