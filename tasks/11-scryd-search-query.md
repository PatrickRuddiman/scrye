Parent slice: [search-engine](../slices/search-engine.md)
Depends on: 09, 04

# Task 11 — scryd-search-query

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `Searcher::search(query)` that runs Witchcraft per mode with `K = max(limit*5, 200)`, and the small custom snippet generator that the api slice will compose with storage filters.

## Tasks
- [x] In `crates/scryd-search/src/searcher.rs`, define `pub struct Searcher { witchcraft: Arc<WitchcraftHandle> }` and constants `K_MIN = 200`, `K_MULTIPLIER = 5`.
- [x] Implement `pub async fn search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError>` that dispatches to `WitchcraftHandle` per `Mode` — full-text via the BM25/FTS5 path, semantic via the XTR path, hybrid via the upstream library's hybrid command. Returns `Hit { message_id, score, semantic_snippet }` for each result. The api slice computes K from `limit`; this function trusts `query.k`.
- [x] In `crates/scryd-search/src/snippet.rs`, expose `pub fn render(body_md: &str, q: &str, max_chars: usize) -> String` per slice §3 Decision 8: case-insensitively find the first occurrence of any whitespace-split term in `q`; return roughly `max_chars` chars centered on it (UTF-8 boundary safe), with `…` ellipsis at non-start/non-end edges, and each matched term wrapped in `**…**`. If no term occurs, return the leading `max_chars` of body_md (also `…`-suffixed if truncated). The api slice composes this with `Hit.semantic_snippet` for the Semantic mode path.
- [x] Write unit tests in `crates/scryd-search/tests/snippet.rs`: query "invoice" against a body containing "the **invoice** is attached" → output centered on the match with `**invoice**` highlighted; query with multiple terms `"invoice acme"` → first occurring term wins; empty query → leading chars; query whose terms don't occur → leading chars; UTF-8 multi-byte body → no panic, valid UTF-8 boundaries.
- [x] Write a unit test asserting `K_MIN = 200` and `K_MULTIPLIER = 5` are exported and used: `let k = std::cmp::max(20 * K_MULTIPLIER, K_MIN); assert_eq!(k, 200);` and `let k = std::cmp::max(50 * K_MULTIPLIER, K_MIN); assert_eq!(k, 250);`.
- [x] Write integration tests in `crates/scryd-search/tests/search.rs` (gated `#[ignore]` if no weights): submit 10 documents through `Drainer`/`WitchcraftHandle`, then run `Searcher::search` in each of the three modes; assert non-empty results in each mode and that `mode=Hybrid` returns a superset of either single-mode result set.

## Acceptance criteria
- [x] `cargo test -p scryd-search --test snippet` passes.
- [x] `cargo check -p scryd-search` exits 0.
- [x] `git grep -nE 'K_MIN\s*:\s*usize\s*=\s*200' crates/scryd-search/src/searcher.rs` matches.
- [x] `git grep -nE 'K_MULTIPLIER\s*:\s*usize\s*=\s*5' crates/scryd-search/src/searcher.rs` matches.
- [x] `git grep -nE 'pub fn render' crates/scryd-search/src/snippet.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
