---
sources:
  - crates/scryd-storage/src/lib.rs
  - crates/scryd-storage/src/db.rs
  - crates/scryd-storage/src/handle.rs
  - crates/scryd-storage/src/migrations/mod.rs
  - crates/scryd-storage/src/migrations/v1_initial.rs
  - crates/scryd-storage/src/migrations/v2_daemon_runs.rs
  - crates/scryd-storage/src/raw.rs
  - crates/scryd-storage/src/threading.rs
  - crates/scryd-storage/src/queue.rs
  - crates/scryd-storage/src/messages.rs
  - crates/scryd-storage/src/accounts.rs
  - crates/scryd-storage/src/sync_state.rs
  - crates/scryd-storage/src/reconcile.rs
---

# Storage

scryd keeps durable state in the data directory (`/var/lib/scryd` under the
systemd unit). This page is the single reference for the on-disk layout and the
`meta.sqlite` schema.

## On-disk layout

| Path | Contents |
| --- | --- |
| `meta.sqlite` | Message metadata, accounts, sync state, the index queue, and daemon-run history. |
| `witchcraft.sqlite` | The search index. See [Indexing and search](indexing-and-search.md). |
| `raw/` | Original `.eml` files (below). |
| `assets/` | witchcraft model weights. See [Weights and assets](../operations/weights-and-assets.md). |

`meta.sqlite` opens for read-write with `journal_mode = WAL`, `synchronous =
NORMAL`, and `foreign_keys = ON`, plus a small read-only connection pool for
concurrent queries.

## meta.sqlite schema

The schema is built by ordered migrations. v1 creates the core tables; v2 adds
the daemon-run journal and a per-queue-row failure timestamp.

### accounts

Mirror of the configured, email-scoped accounts, seeded at startup so message
foreign keys resolve.

| Column | Type | Notes |
| --- | --- | --- |
| `account_id` | TEXT | Primary key. |
| `host` | TEXT | IMAP host. |
| `port` | INTEGER | IMAP port. |
| `username` | TEXT | IMAP login. |
| `folders_json` | TEXT | Configured folders, JSON-encoded. |
| `active` | INTEGER | 1 when currently served. |
| `mirrored_at` | INTEGER | Unix time the row was last reconciled. |

### sync_state

Per-(account, folder) sync progress and health.

| Column | Type | Notes |
| --- | --- | --- |
| `account_id` | TEXT | Part of the primary key; FK to `accounts`. |
| `folder` | TEXT | Part of the primary key. |
| `uidvalidity` | INTEGER or null | Last seen `UIDVALIDITY`. |
| `last_seen_uid` | INTEGER | Highest UID indexed; default 0. |
| `last_full_sync_at` | INTEGER or null | Unix time of the last full sync. |
| `last_idle_at` | INTEGER or null | Unix time the folder last entered IDLE. |
| `last_error` | TEXT or null | Last error string. |
| `account_health` | TEXT | Connection health; default `unknown`. See [IMAP sync](imap-sync.md#account-health). |
| `backoff_until` | INTEGER or null | Unix time the retry backoff ends. |

### messages

One row per fetched message.

| Column | Type | Notes |
| --- | --- | --- |
| `message_id` | TEXT | Primary key (scryd's internal id). |
| `account_id` | TEXT | FK to `accounts`. |
| `folder` | TEXT | Source folder. |
| `server_uid` | INTEGER | IMAP UID. |
| `uidvalidity` | INTEGER | `UIDVALIDITY` at fetch time. |
| `header_message_id` | TEXT or null | RFC 5322 `Message-ID`. |
| `in_reply_to` | TEXT or null | `In-Reply-To` header. |
| `references_json` | TEXT or null | `References` header, JSON-encoded. |
| `thread_id` | TEXT | Thread assignment (below). |
| `sender_addr` | TEXT | From address. |
| `sender_name` | TEXT or null | From display name. |
| `recipients_to_json` | TEXT or null | `To` addresses, JSON-encoded. |
| `recipients_cc_json` | TEXT or null | `Cc` addresses, JSON-encoded. |
| `subject` | TEXT or null | Subject. |
| `date_unix` | INTEGER | Message date, unix seconds. |
| `raw_path` | TEXT | Path to the raw `.eml` (below). |
| `body_md` | TEXT | Markdown body. See [MIME and Markdown](mime-and-markdown.md). |
| `size_bytes` | INTEGER | Raw message size. |
| `tombstoned_at` | INTEGER or null | Unix time the message was found deleted server-side. |

A `UNIQUE (account_id, folder, server_uid, uidvalidity)` constraint dedupes
re-fetches, and indexes cover date, thread, sender, folder, and account access
patterns.

### attachments

Per-message attachment metadata (`attachment_id`, `message_id`, `filename`,
`mime_type`, `size_bytes`). The v1 MCP surface does not return attachment entries;
see [get_message](../mcp/tools/get_message.md).

### index_queue

Drives the index drainer.

| Column | Type | Notes |
| --- | --- | --- |
| `message_id` | TEXT | Primary key. |
| `attempts` | INTEGER | Index attempts so far. |
| `last_error` | TEXT or null | Last index error. |
| `queued_at` | INTEGER | Unix time enqueued. |
| `failed_permanent` | INTEGER | 1 once retries are exhausted. |
| `last_failed_at` | INTEGER or null | Unix time of the last failure (v2). |

Retry and permanent-failure semantics live with the
[drainer](indexing-and-search.md#the-index-drainer).

### daemon_runs

One row per `scryd` process (`run_id`, `started_at`, `stopped_at`, `clean`,
`version`, `pid`). The accounting and crash-loop model are documented in
[Daemon lifecycle](lifecycle.md#run-accounting).

## Raw message files

Each message's original bytes are written under the `raw/` tree at:

```
<data_dir>/raw/<account_id>/<YYYY>/<MM>/<sha256-hex(message_id)>.eml
```

`YYYY`/`MM` come from the message date in UTC. Writes are durable: bytes go to a
temporary file with mode `0600`, are `fsync`ed, then atomically renamed into
place. The MCP [`get_raw_message`](../mcp/tools/get_raw_message.md) tool serves
these files.

## Threading

Each message is assigned a `thread_id` by walking its `References` (newest first)
and then its `In-Reply-To` header. The first referenced id that matches an
existing message's `header_message_id` in the same account reuses that message's
`thread_id`; if nothing matches, a fresh UUID is minted. Threading is by
reference headers only — there is no subject-based grouping.

## See also

- [Architecture](architecture.md)
- [Daemon lifecycle](lifecycle.md)
- [Indexing and search](indexing-and-search.md)
- [MIME and Markdown](mime-and-markdown.md)
- [get_raw_message tool](../mcp/tools/get_raw_message.md)
