---
sources:
  - crates/scryd-search/src/lib.rs
  - crates/scryd-search/src/witchcraft_handle.rs
  - crates/scryd-search/src/indexer.rs
  - crates/scryd-search/src/in_memory.rs
  - crates/scryd-search/src/drainer.rs
  - crates/scryd-search/src/searcher.rs
  - crates/scryd-search/src/snippet.rs
  - crates/scryd-search/src/document.rs
---

# Indexing and search

scryd indexes every stored message and answers queries against that index. This
page covers the index engine, the search modes, how the index queue is drained,
and how candidate counts are sized.

## Engines

Two indexers exist behind one `Indexer` trait:

- **WitchcraftIndexer** is the production engine, backed by `dropbox/witchcraft`.
  It is compiled in on every Unix target (Linux and macOS), because the upstream
  toolchain (candle, fbgemm-rs) is Unix-only for scryd's purposes. It provides
  both a T5 XTR multi-vector semantic index and a full-text index. There is no
  feature flag: semantic search is a core feature, not opt-in.
- **InMemoryIndexer** is a test stub available on all targets so unit tests and
  development on non-Unix hosts can exercise the traits without the ML backend.

The witchcraft index persists to `witchcraft.sqlite` in the data directory,
alongside `meta.sqlite`. See [Storage](storage.md).

## Search modes

A query runs in one of three modes (`Mode`, a closed set):

| Mode | String | Behavior |
| --- | --- | --- |
| Full text | `fulltext` | Lexical match over message text. The default. |
| Semantic | `semantic` | Embedding similarity over the T5 XTR vectors. |
| Hybrid | `hybrid` | Combines full-text and semantic ranking. |

`fulltext` is the default when a query does not specify a mode. The MCP
[`search`](../mcp/tools/search.md) tool exposes these values directly.

For full-text and hybrid hits the snippet is derived from the stored Markdown
body (`body_md`); for semantic hits witchcraft can supply the matching
chunk-region instead. The snippet length the MCP layer returns is documented with
the [`search`](../mcp/tools/search.md) tool.

## The index drainer

Indexing is asynchronous. The IMAP layer enqueues each new message into
`meta.sqlite.index_queue`, and a long-lived drainer task feeds the indexer
(`drainer.rs`):

- It pulls up to `INDEXER_BATCH` (32) rows per loop iteration.
- When the queue is empty and no enqueue notification has arrived, it polls every
  `IDLE_POLL` (1 second) so it still makes progress if a notification is missed.
- After `INDEX_REBUILD_THRESHOLD` (256) embedded documents accumulate without an
  idle flush, it forces an index cascade so clustering does not get too stale
  during a long backfill.
- A row that fails indexing is retried up to `MAX_ATTEMPTS` (5) times. On the
  fifth failure it is flagged `failed_permanent`, removed from the drain
  rotation, and a single-message indexer-failure event is logged. See
  [Observability](observability.md).

The `status` tool surfaces queue depth, the permanent-failure count, and the most
recent index error. See [status](../mcp/tools/status.md).

## Candidate sizing

The MCP layer asks the indexer for more candidates than the caller's `limit` so
post-retrieval account scoping still has hits to choose from. It computes:

```
k = max(limit * K_MULTIPLIER, K_MIN)
```

with `K_MULTIPLIER` = 5 and `K_MIN` = 200. The searcher itself does not adjust
`k`; the caller sets it.

## Errors

`IndexError` (upstream witchcraft error or I/O) and `SearchError` (upstream
witchcraft error) are the engine's error types. A search failure surfaces to MCP
clients as `internal_error`. See the [MCP interface](../mcp/README.md).

## See also

- [Architecture](architecture.md)
- [Storage](storage.md)
- [search tool](../mcp/tools/search.md)
- [Observability](observability.md)
