Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — search-engine

## §1 Summary

Owns indexing every parsed mail message into one Witchcraft database per instance, executing the three search modes the spec promises (full-text, semantic, hybrid), and rebuilding the index in place when the operator runs the reindex CLI verb. Consumed by the api slice (which translates HTTP requests into search calls) and fed by the imap-sync slice (which delivers parsed messages).

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External dependency referenced for design grounding: `dropbox/witchcraft` on GitHub (master branch, primary language Rust, Apache-2.0). Recon notes from the upstream README and repo metadata:

- Pure Rust crate; no crates.io release exists.
- A from-scratch reimplementation of Stanford's XTR-Warp semantic search engine, backed by a single SQLite database.
- Provides three search modes from one library: BM25 via SQLite FTS5 (full-text), XTR multi-vector retrieval (semantic), and a built-in hybrid that combines both. Hybrid is exposed by a CLI verb named `hybrid` and is described in the README verbatim as "combines semantic search with the BM25 search functionality that comes standard with sqlite".
- Indexer is resumable: aborting and restarting the indexer continues where it left off.
- Library-usage template lives at `examples/pickbrain/` in the witchcraft repo. Pickbrain ingests AI-session transcripts/docs and demonstrates the embed → index → query / hybrid loop we mirror.
- Compute-backend feature flags exist (`t5-quantized` default, `t5-openvino`, `metal`, `fbgemm`, `hybrid-dequant`, `embed-assets`, `napi`, `progress`). Selection is a packaging concern — see §6.
- Public Rust API surface beyond what the README shows is not documented in the README; the integration is assumed to follow `examples/pickbrain/`'s pattern, with caveats captured in §7.

## §3 Decisions

1. **Search modes coverage.** Use Witchcraft for full-text, semantic, and hybrid; do not run a second full-text engine. Rationale: README delivers all three on one SQLite DB; smallest dep surface; no two-engine state divergence.
2. **Witchcraft dependency form.** Cargo git dependency on `dropbox/witchcraft` master, pinned to a specific commit hash in `Cargo.toml`. Rationale: simplest "use as a dependency" path; no crates.io release exists; commit pin gives reproducible builds. Escalate to a vendored submodule under `vendor/witchcraft/` only if we need to carry patches we can't upstream.
3. **Database split per instance.** Two SQLite files inside the instance's data directory: `meta.sqlite` (scryd-owned: accounts, folders, sync state, message metadata, threading, work queue) and `witchcraft.sqlite` (Witchcraft-owned). Rationale: schema separation; Witchcraft owns its layout entirely; we never touch its tables.
4. **Indexing pipeline.** Decoupled via a SQLite-backed work queue table in `meta.sqlite`. The IMAP sync worker writes Markdown body + metadata + raw bytes to disk, then enqueues a row in `index_queue`. A separate indexer task drains the queue and calls Witchcraft per message. Rationale: sync runs at IMAP speed; an indexer failure on one message does not stall the sync of the account, satisfying the spec's per-account isolation and indexer-isolation promises.
5. **Reindex strategy.** In-place rebuild using Witchcraft's resumable indexer. The operator's `reindex` CLI verb (a) wipes the Witchcraft DB by calling `index_truncate` and (b) re-enqueues every locally cached message into `index_queue`. The indexer task drains the queue and rebuilds from scratch. Rationale: search continues to serve against whatever portion of the corpus is currently indexed — same continuous-availability behavior the spec already promises during initial backfill, no atomic-swap machinery needed. The spec was updated alongside this decision to remove `/reindex` from the API surface and weaken its rebuild promise to "search continues to serve against the rebuilding index".
6. **Document text submitted to Witchcraft.** Concatenation of `<subject>\n\n<from-display>\n\n<body-markdown>`. Rationale: gets subject and sender content into FT and semantic indexes without per-field weighting machinery; structured filters apply via SQL after retrieval (Decision 9).
7. **Indexing granularity.** One Witchcraft document per email message, keyed by the scryd-internal message id. Rationale: Witchcraft handles internal chunking — the README explicitly states "no chunking strategy" is needed by the caller. Per-message granularity is the natural unit for retrieval and citations. Empty-body messages still register a document so subject hits are findable.
8. **Snippet source per mode.** Full-text and hybrid: a small custom snippet function reads `meta.sqlite.messages.body_md`, finds the first occurrence of any query term (case-insensitive), returns ~200 chars centered on that match with the matched terms wrapped for highlight; falls back to the body's leading 200 chars if no term occurs verbatim. Semantic-only: the matching chunk-region text Witchcraft returns on the hit. Rationale: storage slice keeps `body_md` inline on the messages row; running our own substring snippet avoids carrying a duplicate FTS5 index in `meta.sqlite` purely for the snippet() function.
9. **Filter strategy.** Post-retrieval. The api slice asks `scryd-search` for top-K with `K = max(limit * 5, 200)`; the api slice then SQL-joins returned `message_id`s against `meta.sqlite.messages` and applies `from`, `since`, `until`, `folder`, `account` filters before trimming to `limit`. Rationale: simplest implementation; K is tuned empirically. If selectivity exposes K too low (filters drain results), Decision 9 escalates to pre-retrieval id-restriction — captured as an open question in §7.
10. **Backend feature flags and T5 weights distribution.** Decided in the build-and-packaging slice. The search-engine slice cares only that Witchcraft is callable from Rust at runtime with a working compute backend and a resolvable assets path. Rationale: compute-backend tuning and weight bundling are packaging concerns that span every slice that links Witchcraft, not just this one.

## §4 Contracts & shapes

Internal Rust crate (provisional name): `scryd-search`. Its actual workspace placement lives in build-and-packaging; the contract below is what every other slice consumes regardless of crate layout.

Public surface of `scryd-search`:

- `MessageId` — opaque newtype around a stable string id assigned by the storage slice; used as both the `meta.sqlite.messages` primary key and the Witchcraft document id.
- `Mode` — enum with variants `FullText`, `Semantic`, `Hybrid`.
- `IndexSubmit { message_id: MessageId, document: String }` — the one-shot value the indexer task hands to `scryd-search`.
- `index_submit(&self, submit: IndexSubmit) -> Result<(), IndexError>` — synchronous insert into `witchcraft.sqlite`. Called by the indexer task per drained queue row.
- `index_remove(&self, message_id: MessageId) -> Result<(), IndexError>` — drops the document for a server-side-vanished message. Behavior detail in §7 if Witchcraft does not expose direct removal.
- `index_truncate(&self) -> Result<(), IndexError>` — wipes the Witchcraft DB. Used by the reindex CLI verb dispatch.
- `SearchQuery { q: String, mode: Mode, k: usize }` — what the api slice passes to `search`.
- `SearchResponse { hits: Vec<Hit> }` where `Hit { message_id: MessageId, score: f32, semantic_snippet: Option<String> }`. The api slice fills in metadata, sender, date, folder, and the FTS5-derived snippet from `meta.sqlite.messages` after this returns.
- `search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError>` — entry point.

Per-instance on-disk layout under the instance's data directory `<data>/`:

- `<data>/meta.sqlite` — scryd metadata + work queue (owns `messages.body_md` as inline TEXT — see storage slice).
- `<data>/witchcraft.sqlite` — Witchcraft's database, opened by Witchcraft itself when constructed.
- `<data>/raw/<account_id>/<yyyy>/<mm>/<sha256-hex-of-MessageId>.eml` — original bytes (owned by storage slice).
- T5 assets path — supplied to Witchcraft at construction; resolved by build-and-packaging.

`meta.sqlite.index_queue` shape (this slice owns the row lifecycle; the storage slice owns the schema migration):

- `message_id TEXT PRIMARY KEY`
- `attempts INTEGER NOT NULL DEFAULT 0`
- `last_error TEXT NULL`
- `queued_at INTEGER NOT NULL` — unix epoch seconds at first enqueue.
- `failed_permanent INTEGER NOT NULL DEFAULT 0`

Constants this slice owns:

- `MAX_ATTEMPTS = 5`. Beyond this, the row is marked `failed_permanent = 1`, an entry is logged in the `single-message-{full-text|semantic}-indexer-failure` category from the spec's closed set, and the row is skipped by the drainer.
- `K_MIN = 200`. Floor for Witchcraft `k`.
- `K_MULTIPLIER = 5`. `k = max(request.limit * 5, K_MIN)`.
- `INDEXER_BATCH = 32`. Number of rows the drainer pulls per loop iteration.

## §5 Sequence

1. **Initial-backfill ingest.** IMAP sync (imap-sync slice) parses message → mime-and-markdown slice produces Markdown body and attachment metadata → storage slice writes `meta.sqlite.messages` row (with `body_md` inline) + `raw/.../<sha256-hex>.eml` → storage inserts `index_queue` row.
2. **Indexer drain loop.** A long-lived task in `scryd-search` repeats: select up to `INDEXER_BATCH` rows from `index_queue` ordered by `queued_at` ascending where `failed_permanent = 0`, for each row read `subject`, `sender_addr`/`sender_name`, and `body_md` from `meta.sqlite.messages`, build the document text per Decision 6, call `Witchcraft.index(document, message_id)`. On Ok, delete the queue row. On Err, increment `attempts`, set `last_error`; if `attempts >= MAX_ATTEMPTS`, set `failed_permanent = 1` and emit the failure log line. If the queue is empty, sleep until storage signals "queue not empty" or a poll timer fires.
3. **Search query.** API request lands in api slice → handler validates filters → calls `scryd-search.search(SearchQuery { q, mode, k = max(limit * 5, K_MIN) })` → handler SQL-joins returned `message_id`s against `meta.sqlite.messages` applying the request's filters → trims to `limit` → for each surviving hit, fills in subject/from/date/folder/account and derives the snippet (FTS5 `snippet()` against the body column for FullText/Hybrid, `Hit.semantic_snippet` for Semantic) → returns the assembled response.
4. **Reindex (CLI only).** Operator runs the reindex CLI verb (cli slice) → CLI dispatches a "reindex" command to the running instance over the local IPC channel (owned by multi-instance-isolation slice) → daemon calls `scryd-search.index_truncate()` → daemon copies every `meta.sqlite.messages.message_id` into `index_queue` resetting `attempts = 0` and `failed_permanent = 0` → indexer drain loop (step 2) rebuilds. Search continues to serve from `witchcraft.sqlite` throughout; result counts grow back as the rebuild progresses.
5. **Server-side message disappears.** IMAP sync detects a UID is gone → storage marks the message tombstoned → storage emits a remove signal that the indexer task picks up → calls `scryd-search.index_remove(message_id)` → message no longer returned by search.

## §6 Out of scope

- HTTP request/response shape for `/search`, `/message/:id`, `/thread/:id`, `/accounts`, `/sync` (api slice).
- Per-user IPC channel that carries the CLI's reindex command into the running daemon (multi-instance-isolation slice).
- The `meta.sqlite` schema for accounts/folders/messages/threads (storage slice). This slice references `index_queue`; the storage slice authors and migrates it.
- MIME parsing, HTML→Markdown conversion, attachment metadata extraction, threading reconstruction (mime-and-markdown slice).
- IMAP sync, IDLE/poll, UID/UIDVALIDITY tracking, per-account parallelism (imap-sync slice).
- Witchcraft compute-backend feature flags, T5 weights distribution, `embed-assets` vs disk path, the assets path passed at Witchcraft construction (build-and-packaging slice).
- Log target wiring and severity levels (observability slice). This slice declares which failure categories it emits; the slice that owns the system-log target shapes the writes.
- Per-arch CI build matrix (build-and-packaging slice).

## §7 Open questions

- Witchcraft's public Rust API for single-document removal (`index_remove` shape) is not documented in the README. Confirm by reading `examples/pickbrain/` source on the commit we pin; if direct removal is unsupported, this slice maps `index_remove` to a tombstone marker plus next-reindex compaction.
- Witchcraft semantic-mode result shape: confirm it surfaces the matching chunk-region text per hit so we can populate `Hit.semantic_snippet` directly. If not, semantic snippets fall back to an FTS5 `snippet()` derived excerpt of the body Markdown around the message id.
- Pre-retrieval id-restriction: confirm whether Witchcraft's query API accepts a caller-supplied allow-list of document ids. If yes, Decision 9 gains a fast path for selective filters; if no, K-tuning is the only lever.
- Witchcraft thread-safety: confirm whether one `Witchcraft` instance can be shared across the indexer task and the search request handlers, or whether we need separate handles. The pickbrain example will tell us; the spec's "search available during backfill" promise depends on concurrent read while another task writes.
- The exact dropbox/witchcraft commit hash to pin against in `Cargo.toml` is intentionally not chosen in this doc — it gets set when the first task here begins coding and is bumped explicitly thereafter. **Update from task 09 execution:** master at `1370cd569c7a882815cc22169f52dc9aa195cda7` (2026-05-05) pins `rusqlite 0.35` (libsqlite3-sys 0.33), which collides with scryd-storage's `rusqlite 0.31` (libsqlite3-sys 0.28) under Cargo's `links = "sqlite3"` uniqueness rule — even when witchcraft is gated behind an optional feature. Resolving requires either (a) bumping scryd-storage to rusqlite 0.35 (and absorbing any 0.31→0.35 API drift) or (b) waiting for upstream witchcraft to bump down. Until then, scryd-search ships `InMemoryIndexer` against an `Indexer` trait so tasks 10/11/18/19 unblock; the production binding lands in a follow-up named `scryd-search-witchcraft-binding-v2`.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
