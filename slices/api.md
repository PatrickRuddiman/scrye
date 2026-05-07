Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — api

## §1 Summary

Owns the HTTP-over-Unix-socket surface that callers use to reach a running scryd instance: the six public operations the spec enumerates (search, get parsed message, get raw bytes, get thread, list accounts, trigger sync), plus a small set of internal CLI-only operations (reindex, reconcile-config) that the cli slice dispatches over the same socket. Pins request/response shapes, status-code mapping, and behavior during backfill / reindex.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External Rust crates this slice leans on:

- `axum` — HTTP framework. Spec named it. Native tokio integration; first-class Unix-domain-socket support via `tokio::net::UnixListener` + `axum::serve`.
- `serde` / `serde_json` — request/response serialization.
- `tokio` — async runtime.

## §3 Decisions

1. **Transport.** HTTP/1.1 over the Unix domain socket multi-instance-isolation owns at `$XDG_RUNTIME_DIR/scryd/scryd.sock`. No TCP listener; no HTTP/2. Rationale: HTTP/2 over UDS is rarely supported by clients (curl needs `--unix-socket` + `--http1.1`); HTTP/1.1 keeps client compatibility universal. The OS-level identity check happens before any axum code runs.
2. **Two endpoint classes.**
   - **Public** (the spec's closed-set operations): `GET /search`, `GET /message/:id`, `GET /message/:id/raw`, `GET /thread/:id`, `GET /accounts`, `POST /sync`. Stable between releases.
   - **Internal** (CLI dispatch only; not part of the spec's closed-set promise): `POST /internal/reindex`, `POST /internal/reconcile`. Documented as "not part of the public API contract; may change between releases; reachable by any same-uid caller because the kernel-level identity check is the only gate". Rationale: the spec promises that reindex is "only invocable through the operator's CLI" — the CLI is the only documented client; the path under `/internal/` makes the contract status visible without re-introducing application-level auth.
3. **Response shapes are flat JSON.** Successful responses are top-level objects (no `{ok: true, data: ...}` envelope). Errors return non-2xx status with `{"error": {"code": "<symbol>", "message": "<human-readable>"}}`. Rationale: simpler for shell-script callers; ergonomic for `jq`; status code already carries success/failure.
4. **Synchronous shape for `POST /sync`.** Returns `202 Accepted` once the sync pass has *begun* (the imap-sync slice has been signaled to run a pass and has acknowledged). Body: `{"started": true, "scope": {"accounts": ["primary", …]}}`. Rationale: spec §5 open question listed sync-async vs sync-sync as needing confirmation; the CLI iteration kept "returns once the pass has begun"; that's what we ship.
5. **Atomic-swap reindex is gone.** `POST /internal/reindex` triggers an in-place rebuild via storage + search-engine slices: response is `202 Accepted` with `{"started": true}` once `index_truncate` + queue-refill has completed. The rebuild itself runs in the indexer task. Search continues to serve from `witchcraft.sqlite` throughout. Rationale: reflects the spec's updated promise (search continues against the rebuilding index, no atomic-swap machinery).
6. **No "ready" gate.** Search and message-fetch handlers are wired up the moment the daemon binds the socket. Empty-corpus and partial-corpus responses are first-class success cases (the spec promises this explicitly). Rationale: spec acceptance is "search succeeds even during first-run backfill, returning whatever's been indexed so far".
7. **Filter validation.** `q` may be empty (returns most-recent messages by date desc). `from` is a substring match against `messages.sender_addr` (lowercased). `since` and `until` accept ISO-8601 dates (`YYYY-MM-DD`) — converted to unix seconds at request boundary. `folder` matches `messages.folder` exactly (case-sensitive — IMAP folders are case-sensitive). `account` matches `accounts.account_id` exactly. `limit` is `1..=200` (default 20; clamped, not rejected). `mode` is `fulltext` (default) | `semantic` | `hybrid`. Rationale: every filter the spec enumerates is honored; clamping `limit` instead of rejecting prevents trivial DOS via huge limits.
8. **Search hit shape.** Each hit returns: `message_id`, `account_id`, `folder`, `sender_addr`, `sender_name`, `subject`, `date` (ISO-8601), `score` (f32), `snippet` (string, ≤ 240 chars), `thread_id`. Rationale: enough for a caller to render a result list and chase into `/message/:id` for the body; matches the storage row plus snippet.
9. **Message-fetch shape.** Returns the spec's exhaustive parsed-message field set, with addresses as `{addr, name?}` objects, `attachments` as `[{filename, mime_type, size_bytes}]`, `body` as a Markdown string, `references` as a list of strings. Rationale: 1:1 with the spec's closed-set promise on parsed-message responses.
10. **Raw bytes.** `GET /message/:id/raw` streams the file from disk via tokio's `ReaderStream`, with `Content-Type: message/rfc822` and `Content-Length` from the file metadata. Rationale: emails are sometimes multi-MB; streaming avoids loading the whole file into memory; `message/rfc822` is the IANA-registered type for `.eml`.
11. **Thread shape.** Returns `{thread_id, messages: [parsed-message, …]}` with messages sorted oldest-first per the spec. Rationale: spec is explicit; including `thread_id` lets the caller paginate or display the thread title.
12. **Accounts shape.** `{accounts: [{account_id, folders: [string, …]}, …]}`. No credential, no auth state, no sync timestamp. Rationale: matches the spec's closed-set list of fields for list-accounts; deliberately empty of dynamic state because health belongs in system logs.
13. **Status code map.** Closed set:
    - `200` — successful search / fetch / list response.
    - `202` — accepted-but-pending: `POST /sync`, `POST /internal/reindex`, `POST /internal/reconcile`.
    - `400` — `bad_query`: malformed parameters (invalid date, unknown mode, empty path component).
    - `404` — `not_found`: unknown `message_id` or `thread_id`.
    - `409` — `conflict`: a reindex is already in progress when another reindex is requested.
    - `503` — `shutting_down`: the daemon is draining for SIGTERM.
    - `500` — `internal_error`: anything else (logged with full context, body suppresses stack info).
14. **No CORS, no compression, no body parsing for GETs.** The transport is local-only; clients are scripts/agents. Rationale: every byte we don't process is a byte we don't have a bug in.
15. **Concurrent handler behavior.** Search/message-fetch/thread/account handlers acquire connections from storage's read-only pool. `POST /sync` and `POST /internal/reindex` enqueue an action and return immediately, without blocking inflight reads. `POST /internal/reconcile` triggers a write through the imap-sync scheduler's `reconcile()` and returns once the new account list is mounted. Rationale: spec promises search availability throughout sync, reindex, and config changes — the handlers must not lock readers behind writers.
16. **One in-flight reindex.** A daemon-wide `Mutex<Option<ReindexHandle>>` ensures at most one reindex runs at a time. A second `POST /internal/reindex` while one is running returns `409 conflict`. Rationale: multiple concurrent rebuilds would shred the indexer task and corrupt search results' growing visibility.
17. **No rate limiting in v1.** Local-only API on a per-user socket; the only realistic abuser is the operator's own runaway script. Rationale: simplest correct behavior; revisit if a v2 caller pattern (e.g., a chat agent firing thousands of queries) demands it.

## §4 Contracts & shapes

Internal Rust crate (provisional): `scryd-api`.

Endpoint catalogue:

**`GET /search`**
- Query parameters: `q` (string, optional, default `""`), `from` (string, optional), `since` (ISO-8601 date, optional), `until` (ISO-8601 date, optional), `folder` (string, optional), `account` (string, optional), `limit` (1..=200, default 20), `mode` (`fulltext`|`semantic`|`hybrid`, default `fulltext`).
- 200 body:
  ```
  {
    "hits": [
      {
        "message_id": "primary:<…>",
        "account_id": "primary",
        "folder": "INBOX",
        "sender_addr": "alice@example.com",
        "sender_name": "Alice",
        "subject": "Re: invoice March",
        "date": "2026-04-30T14:21:00Z",
        "score": 12.7,
        "snippet": "…the **invoice** is attached…",
        "thread_id": "9f3c…"
      }
    ],
    "mode": "fulltext",
    "elapsed_ms": 18
  }
  ```
- 400 on malformed `since`/`until`/`mode`/`limit`.

**`GET /message/:id`**
- Path: URL-encoded `message_id`.
- 200 body: `{message_id, account_id, folder, header_message_id, in_reply_to, references: [...], thread_id, from: {addr, name?}, to: [...], cc: [...], subject, date, body_md, attachments: [{filename, mime_type, size_bytes}]}`.
- 404 if not found OR if tombstoned.

**`GET /message/:id/raw`**
- Path: URL-encoded `message_id`.
- 200 body: streamed bytes; headers `Content-Type: message/rfc822`, `Content-Length: <size_bytes>`.
- 404 if not found OR if tombstoned.

**`GET /thread/:id`**
- Path: URL-encoded `thread_id`.
- 200 body: `{thread_id, messages: [<parsed-message-shape>, ...]}` ordered oldest-first.
- 404 if no non-tombstoned messages exist for the thread.

**`GET /accounts`**
- 200 body: `{accounts: [{account_id, folders: [string, ...]}, ...]}`. Always includes `accounts.active = 1` rows only.

**`POST /sync`**
- No request body.
- 202 body: `{started: true, scope: {accounts: [...]}}` listing every account a pass was started for.

**`POST /internal/reindex`**
- No request body.
- 202 body: `{started: true}` after `index_truncate` and queue-refill have completed (rebuild itself happens in the indexer task).
- 409 body if a reindex is already in progress.

**`POST /internal/reconcile`**
- No request body.
- 202 body: `{started: true, accounts_added: [...], accounts_inactivated: [...]}` once the imap-sync scheduler has applied the new account list.

Error envelope (any non-2xx):
```
{"error": {"code": "bad_query"|"not_found"|"conflict"|"shutting_down"|"internal_error", "message": "<human-readable>"}}
```

Crate boundaries `scryd-api` depends on:

- `scryd-storage::ReadHandle` for query/fetch (storage slice).
- `scryd-storage::WriteHandle` for reconcile triggers (storage slice).
- `scryd-search::Searcher` for `search` (search-engine slice).
- `scryd-search::Indexer::truncate()` + `scryd-storage::reenqueue_all()` for reindex (search-engine + storage slices).
- `scryd-imap::Scheduler` for `sync` and `reconcile` triggers (imap-sync slice).
- `scryd-mime` is not a direct dep; storage owns the MIME pipeline.

Constants:

- `LIMIT_MAX = 200`
- `LIMIT_DEFAULT = 20`
- `SNIPPET_MAX_CHARS = 240`
- `SNIPPET_TERM_HIGHLIGHT_OPEN = "**"`, `SNIPPET_TERM_HIGHLIGHT_CLOSE = "**"` (Markdown bold; matches body_md format).

## §5 Sequence

1. **Daemon start.** multi-instance-isolation slice opens the socket → api slice mounts axum routes onto it via `axum::serve(unix_listener, router)`.
2. **GET /search.** Handler validates filters → calls `scryd-search.search(SearchQuery { q, mode, k = max(limit*5, 200) })` → joins returned `message_id`s against storage with the request's filters (`from`, `since`, `until`, `folder`, `account`, `tombstoned_at IS NULL`) → trims to `limit` → for each surviving hit, derives the snippet (custom function over `body_md` for FullText/Hybrid; semantic_snippet from Witchcraft for Semantic) → serializes the response.
3. **GET /message/:id.** Handler fetches the row from `messages` and rows from `attachments` via storage's read pool → if `tombstoned_at IS NOT NULL` returns 404 → otherwise serializes the parsed-message shape.
4. **GET /message/:id/raw.** Handler fetches `raw_path` from `messages` → opens the file → streams with `Content-Type: message/rfc822`. 404 if row missing or tombstoned.
5. **GET /thread/:id.** Handler queries `SELECT … FROM messages WHERE thread_id = ? AND tombstoned_at IS NULL ORDER BY date_unix ASC` → for each row, serializes parsed-message-shape (loading attachments per row in one batch).
6. **GET /accounts.** Handler queries `SELECT account_id, folders_json FROM accounts WHERE active = 1 ORDER BY account_id` → JSON-decodes `folders_json` into the array → returns.
7. **POST /sync.** Handler calls `imap-sync::Scheduler::request_pass()` (a small new method on the scheduler that signals every account supervisor to "fetch now if you can") → scheduler returns the list of accounts that were signaled (some may be in backoff and skipped) → response 202.
8. **POST /internal/reindex.** Handler acquires the daemon-wide reindex mutex (Decision 16). If already held, returns 409. Otherwise: calls `scryd-search::Indexer::truncate()`, then `scryd-storage::reenqueue_all_messages()`, releases the mutex, returns 202. The indexer task drains the queue independently from this point on.
9. **POST /internal/reconcile.** Handler triggers `scryd-storage::reconcile_accounts_from_config()` → triggers `scryd-imap::Scheduler::reconcile()` → returns 202 with the diff.
10. **SIGTERM.** axum's graceful shutdown is wired to the daemon's shutdown signal. New connections rejected with TCP-style RST-equivalent (close); in-flight requests get to finish; `POST /sync` and `POST /internal/*` started but unfinished work cleanly winds down via the scheduler's own shutdown path.

## §6 Out of scope

- The `meta.sqlite` schema and read-pool implementation (storage slice).
- Witchcraft search execution (search-engine slice).
- IMAP-side semantics (imap-sync slice).
- The Unix socket's lifecycle, permissions, and `SO_PEERCRED` enforcement (multi-instance-isolation slice).
- The CLI's argument parsing and how it dispatches to these endpoints (cli slice).
- Logging the access lines themselves (observability slice). This slice declares which response status codes get logged at what level; observability owns the writes.
- Pagination, cursors, sortable columns beyond `score`/`date` — v2 if needed.
- WebSocket/SSE streaming of new-mail events. v1 has no push surface.
- Rate limiting. None in v1.

## §7 Open questions

- Whether `GET /search` should also accept POST bodies for very long queries (semantic queries can run hundreds of chars). Default v1: GET only with URL-encoded `q`. Most clients fit within typical URL limits (~8KB on the kernel-level UDS path).
- Whether `account` should be repeatable (search a subset of accounts) or single-valued. Default v1: single-valued; absence means "all accounts in this instance". Matches the spec's open question that scoped this to a confirmation.
- Whether `from` should be substring or exact-match. Default v1: substring against `messages.sender_addr`. If the operator sets `from=alice` they get every alice@*; if they want exact they pass the full address.
- Whether `POST /internal/reconcile` should be folded into the daemon's startup self-checks rather than exposed as an endpoint (i.e., the only way to reconcile is to restart). Default v1: keep it as an endpoint so credential rotation doesn't require systemd restart and so multi-account add-account flows are smoother. cli slice will confirm.
- Whether attachment payloads should be retrievable at all (e.g., `GET /message/:id/attachment/:idx`). Spec §3 Out is explicit: only metadata. v1 doesn't add this; v2 may.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
