Parent plan: scryd v0.3.1 — service pivot
Depends on: none

# Task 05 — account-ids-filter

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Add caller-driven multi-account filtering to the search API. New `?account_ids=a,b,c` query param on `GET /search` (and analogous filter on `/thread/:id` and `/accounts`), plus a CLI `--accounts` flag that takes a comma-delimited list. Empty filter = return all accounts (open by default). Implementation post-filters at the API layer; pushing the filter into witchcraft's metadata SQL is left as the v0.3.5 perf optimisation tracked in `tasks/v0.3.1/triage/witchcraft-metadata-account-filter.md`.

## Tasks
- [ ] In `crates/scryd-search/src/lib.rs:94-98` (`SearchQuery`), add `pub account_ids: Vec<String>` (default empty).
- [ ] In `crates/scryd-api/src/dto.rs`, add `account_ids: Option<String>` (comma-delimited) to the `SearchQueryDto` struct used by `handle_search`.
- [ ] In `crates/scryd-api/src/handlers.rs:22-119` (`handle_search`), parse the `account_ids` query param into `Vec<String>` (split on `,`, trim each, drop empties), pass into `SearchQuery`. The existing single-value `account: Option<String>` filter stays for backward compat — a present `account` value is appended to `account_ids` if not already there.
- [ ] In `crates/scryd-api/src/handlers.rs:130-145` (`filter_match`), extend the existing `row.account_id != account` post-filter to: when `account_ids` is non-empty, drop rows whose `account_id` is not in the set. When empty, no filter.
- [ ] In `scryd/src/main.rs:60-79` (`SearchArgs`), add `#[arg(long, value_delimiter = ',')] accounts: Option<Vec<String>>` alongside the existing single-value `account`. `--account foo` and `--accounts foo,bar` both work; the URL builder unions them.
- [ ] In `scryd/src/main.rs:240-260` (`build_search_url`), include `account_ids=foo,bar` when the multi-value flag was supplied.
- [ ] Add `crates/scryd-api/tests/handlers_read.rs::search_filters_to_account_ids_when_supplied`: spin up an in-memory storage with three accounts (a, b, c) each carrying a couple of indexed messages; query `?account_ids=a,c`; assert the returned hits' `account_id`s are all in `{a, c}` and that account `b` is excluded.
- [ ] Add `crates/scryd-api/tests/handlers_read.rs::search_returns_all_accounts_when_filter_empty`: same fixture, query without `account_ids`; assert all three accounts' hits come through.

## Acceptance criteria
- [ ] `cargo test -p scryd-api --test handlers_read` passes (existing tests + the two new ones).
- [ ] `cargo test -p scryd --test cmd_search` passes (the `--accounts` flag round-trips through `build_search_url`).
- [ ] `git grep -nE 'pub account_ids: Vec<String>' crates/scryd-search/src/lib.rs` matches the new field.
- [ ] `git grep -nE 'account_ids' crates/scryd-api/src/handlers.rs | wc -l` returns at least 2 (param parse + filter).
- [ ] `target/debug/scryd search --help 2>&1 | grep -F '--accounts'` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
