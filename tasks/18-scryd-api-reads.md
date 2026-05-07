Parent slice: [api](../slices/api.md)
Depends on: 17, 11, 04, 05

# Task 18 — scryd-api-reads

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the five public read endpoints — `GET /search`, `GET /message/:id`, `GET /message/:id/raw`, `GET /thread/:id`, `GET /accounts` — with the exact field shapes and status codes the api slice fixes.

## Tasks
- [ ] In `crates/scryd-api/src/dto.rs`, define request and response DTOs matching api slice §4 verbatim: `SearchQueryDto`, `SearchHitDto`, `SearchResponseDto`, `MessageDto`, `AddressDto`, `AttachmentDto`, `ThreadResponseDto`, `AccountsResponseDto`, `AccountDto`, `ErrorDto`. Use `chrono` (or `time` crate, pick one and add to deps) to serialize `date_unix` as ISO-8601 UTC and parse `since`/`until` query params from `YYYY-MM-DD`.
- [ ] In `crates/scryd-api/src/handlers/search.rs`, implement `pub async fn handle_search(State(state): State<AppState>, Query(q): Query<SearchQueryDto>) -> Response`. Steps: validate filters (clamp `limit` to `1..=200` default 20; map `mode` string to `Mode`; reject malformed dates with `400 bad_query`); compute `k = max(limit*5, 200)`; call `state.searcher.search(SearchQuery { q.q, mode, k })`; for each hit, look up the message row in storage (`tombstoned_at IS NULL`), apply the `from`/`since`/`until`/`folder`/`account` filters; trim to `limit`; for each surviving hit, derive the snippet (`scryd_search::snippet::render` for FullText/Hybrid; `Hit.semantic_snippet.unwrap_or_else(|| render(...))` for Semantic); serialize.
- [ ] In `crates/scryd-api/src/handlers/message.rs`, implement `pub async fn handle_message(State, Path(id)) -> Response`: storage `get_message(id)` → if `None` or `tombstoned_at IS NOT NULL`, return `404 not_found`; otherwise hydrate addresses (`recipients_to_json`/`recipients_cc_json`), attachments (storage `list_attachments(id)`), serialize.
- [ ] In `crates/scryd-api/src/handlers/raw.rs`, implement `pub async fn handle_raw(State, Path(id)) -> Response`: storage `get_message` → if `None` or tombstoned → `404`; open `raw_path` via `scryd_storage::raw::open_raw`; wrap in `tokio_util::io::ReaderStream`; respond with `Content-Type: message/rfc822`, `Content-Length: size_bytes`, and the streaming body.
- [ ] In `crates/scryd-api/src/handlers/thread.rs`, implement `pub async fn handle_thread(State, Path(id)) -> Response`: storage `get_thread(id)` → if empty, `404`; else for each row build a `MessageDto` (including attachments), wrap in `ThreadResponseDto` ordered oldest-first.
- [ ] In `crates/scryd-api/src/handlers/accounts.rs`, implement `pub async fn handle_accounts(State) -> Response`: storage `list_active_accounts()` → produce `AccountsResponseDto { accounts: [{account_id, folders}] }`. Folders come from `accounts.folders_json`.
- [ ] In `crates/scryd-api/src/router.rs`, mount the five routes: `/search`, `/message/:id`, `/message/:id/raw`, `/thread/:id`, `/accounts`. Wire each to its handler.
- [ ] Add a tower layer that emits the api access log per observability slice §3 Decision 11: on response, `log_request!(method, path, status, duration_ms)`.
- [ ] Write integration tests in `crates/scryd-api/tests/handlers_read.rs` using a tokio + reqwest+unix-socket harness: seed storage with three messages (different senders, dates, folders); start the api on a temp socket; assert `GET /search?q=invoice&limit=2` returns 200 with two hits matching shape; assert `GET /search?since=2099-01-01` returns 200 with empty hits; assert `GET /search?since=garbage` returns 400 `bad_query`; assert `GET /message/<id>` returns the parsed shape; assert `GET /message/<unknown>` returns 404 `not_found`; assert `GET /message/<tombstoned-id>` returns 404; assert `GET /message/<id>/raw` streams `Content-Type: message/rfc822` and the body matches the seeded raw bytes; assert `GET /thread/<id>` returns oldest-first ordering; assert `GET /accounts` returns the configured accounts array.
- [ ] Write a test asserting the access-log layer emits a DEBUG entry per request and a WARN entry on a 400/404.

## Acceptance criteria
- [ ] `cargo test -p scryd-api --test handlers_read` passes.
- [ ] `cargo check -p scryd-api` exits 0.
- [ ] `git grep -nE 'limit\.clamp\(1,\s*200\)|LIMIT_MAX\s*:\s*usize\s*=\s*200' crates/scryd-api/src/handlers/search.rs` matches.
- [ ] `git grep -nE 'message/rfc822' crates/scryd-api/src/handlers/raw.rs` matches.
- [ ] `git grep -nE '"not_found"|"bad_query"' crates/scryd-api/src/handlers/` matches both.
- [ ] `git grep -nE 'log_request!' crates/scryd-api/src/` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
