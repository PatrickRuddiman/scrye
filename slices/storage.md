Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — storage

## §1 Summary

Owns the per-instance on-disk durable state: the `meta.sqlite` schema (accounts, sync state, messages, attachments, threads, indexer work queue), migration runner, raw `.eml` byte store, and the idempotency rules that let sync resume cleanly after a crash. Every other slice that reads or writes durable state goes through this slice's tables.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External primitives this slice leans on: SQLite (WAL journal mode, single-writer + many-readers concurrency, FTS5 not used here — see §3 Decision 11), and the per-user data directory committed by the multi-instance-isolation slice at `$XDG_DATA_HOME/scryd/`.

## §3 Decisions

1. **Single metadata database per instance.** One file at `$XDG_DATA_HOME/scryd/meta.sqlite`, mode `0600`, owned by the operator. Rationale: simplest atomic backup target; one file the operator can copy/restore; aligns with the multi-instance-isolation layout.
2. **WAL journal mode.** Open the DB with `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`. Rationale: concurrent readers (search query handlers, message-fetch handlers) while the IMAP sync worker writes; spec's "search available throughout backfill" depends on this.
3. **Single writer.** Only the IMAP sync worker and the indexer task write. Both serialize through a single async-mutex-guarded SQLite connection. Search/message-fetch use a separate read-only connection pool. Rationale: SQLite WAL is single-writer; coordinating in-process via one connection avoids `SQLITE_BUSY` retries entirely.
4. **MessageId scheme.** `<account_id>:<canonical>`, where canonical is the RFC 5322 Message-ID header stripped of `<>` and lowercased if it parses; otherwise `synth-<hex(sha256(folder ⌃ uidvalidity ⌃ server_uid))>`. Rationale: stable across UIDVALIDITY resets when the header survives (which is the common case); deterministic synthetic fallback for the rare malformed-header case; account-scoped so two accounts that legitimately receive the same message don't collide.
5. **Body stored inline as TEXT** on `messages.body_md`. No separate bodies/ files. Rationale: target corpus (~50K–100K messages) keeps the DB well under operating-system limits; SQLite TOASTs large TEXT into overflow pages so wide rows don't pessimize narrow scans; one source of truth for the snippet path; one fewer escaping/sanitization concern.
6. **Raw bytes stored as files** at `$XDG_DATA_HOME/scryd/raw/<account_id>/<yyyy>/<mm>/<sha256-hex(MessageId)>.eml`, mode `0600`. Rationale: raw bytes (with attachments) can dwarf body text; files leverage the kernel page cache; per-month sharding keeps any one directory bounded; SHA-256-hex filenames sidestep filesystem-illegal characters in headers.
7. **Idempotency.** `UNIQUE(account_id, folder, server_uid, uidvalidity)` on `messages`. Sync uses `INSERT … ON CONFLICT(account_id, folder, server_uid, uidvalidity) DO UPDATE SET …` to merge an updated row, or `DO NOTHING` for plain re-fetch. Rationale: lets a SIGKILL-then-restart re-process the same UID range without dups; merge-update lets us pick up changed flags/headers without dup rows.
8. **Threading at insert.** When a message lands, compute its `thread_id` by walking `References` + `In-Reply-To`: if any referenced header-Message-ID matches an existing `messages.header_message_id` for the same account, reuse that row's `thread_id`; otherwise mint a new `thread_id` (UUID v4). Rationale: thread retrieval becomes a single indexed scan; no graph-walk at query time. Per-account scoping prevents accidental thread collapse across accounts that share quoted threads.
9. **Tombstones, no immediate raw delete.** On server-side disappearance (UID gone after sync), set `messages.tombstoned_at = now`, leave the raw `.eml` file in place. Search and message-retrieval exclude tombstoned rows. Rationale: spec lets us not delete raw bytes immediately (operator-driven cleanup is out of scope for v1); tombstones make "missing on server" recoverable if the operator reconfigures or the server restores.
10. **Migrations.** Forward-only. A `schema_version` table holds the highest applied version. The daemon runs the migration list at startup, before opening the IPC socket; each migration is a small Rust function paired with a SQL string. No down migrations; a botched forward migration is fixed by a follow-up forward migration. Rationale: the operator does not run migrations by hand; one direction simplifies recovery; spec's "daemon refuses to start on schema problems" surfaces as a normal `configuration parse error` log line.
11. **No FTS5 in meta.sqlite.** All BM25/full-text search lives inside `witchcraft.sqlite` (search-engine slice). meta.sqlite is purely a metadata store. Rationale: avoids a duplicate FTS5 index; simplifies schema; snippet derivation runs as a small custom function over `body_md` (search-engine slice updates §3 Decision 8 to match).
12. **Account record mirrors the config file, password excluded.** On daemon start (and after `add-account` writes the config), the daemon reconciles `accounts` rows from `$XDG_CONFIG_HOME/scryd/config.toml`: insert new accounts, update changed host/port/folder lists, mark configured-out accounts inactive (do not delete — preserves `messages` and `sync_state` rows for recovery). The credential **never** lands in `meta.sqlite`. Rationale: config.toml is the only authoritative credential location (multi-instance-isolation slice); meta.sqlite mirrors structure for query purposes.
13. **Per-account-folder sync state** in a separate `sync_state` table keyed by `(account_id, folder)`. Rationale: folder-level UIDVALIDITY/last-seen-UID are the unit IMAP cares about; per-account health rolls up from this table; isolating sync state from message rows lets the imap-sync slice churn it without contention against search readers.

## §4 Contracts & shapes

Per-instance on-disk layout under `$XDG_DATA_HOME/scryd/`:

- `meta.sqlite` — primary metadata DB, mode `0600`.
- `meta.sqlite-wal`, `meta.sqlite-shm` — WAL companions, created by SQLite, mode `0600`.
- `raw/<account_id>/<yyyy>/<mm>/<sha256-hex-of-MessageId>.eml` — original bytes per message, mode `0600`. `<yyyy>/<mm>` derived from the message's `Date` header (or first-seen timestamp if header is missing/invalid).
- `witchcraft.sqlite` and friends — owned by search-engine slice; storage slice does not touch.

`schema_version` table:

- `version INTEGER PRIMARY KEY`
- `applied_at INTEGER NOT NULL`

`accounts` table:

- `account_id TEXT PRIMARY KEY` — operator-chosen identifier from config.
- `host TEXT NOT NULL`
- `port INTEGER NOT NULL`
- `username TEXT NOT NULL`
- `folders_json TEXT NOT NULL` — JSON array of folder paths to sync.
- `active INTEGER NOT NULL DEFAULT 1` — 0 if removed from config but rows retained.
- `mirrored_at INTEGER NOT NULL` — unix epoch seconds at last config reconciliation.

No password column. The credential is read from `$XDG_CONFIG_HOME/scryd/config.toml` at sync time only.

`sync_state` table:

- `account_id TEXT NOT NULL`
- `folder TEXT NOT NULL`
- `uidvalidity INTEGER NULL` — null until first SELECT/EXAMINE.
- `last_seen_uid INTEGER NOT NULL DEFAULT 0`
- `last_full_sync_at INTEGER NULL` — unix epoch seconds.
- `last_idle_at INTEGER NULL`
- `last_error TEXT NULL`
- `account_health TEXT NOT NULL DEFAULT 'unknown'` — one of: `unknown`, `active`, `degraded`, `auth-rejected`, `quota-exceeded`, `unreachable`, `tls-failed`. Set by imap-sync slice.
- `backoff_until INTEGER NULL` — unix epoch seconds; sync skips this folder until reached.
- `PRIMARY KEY (account_id, folder)`
- `FOREIGN KEY (account_id) REFERENCES accounts(account_id)` (no cascade — keep history if account inactivated).

`messages` table:

- `message_id TEXT PRIMARY KEY` — the scryd-internal MessageId from Decision 4.
- `account_id TEXT NOT NULL`
- `folder TEXT NOT NULL`
- `server_uid INTEGER NOT NULL`
- `uidvalidity INTEGER NOT NULL`
- `header_message_id TEXT NULL` — RFC 5322 Message-ID, lowercased, `<>`-stripped, or NULL if absent/malformed.
- `in_reply_to TEXT NULL`
- `references_json TEXT NULL` — JSON array of normalized header-Message-IDs.
- `thread_id TEXT NOT NULL` — UUID v4 minted at insert per Decision 8.
- `sender_addr TEXT NOT NULL` — `addr-spec` portion of `From`, lowercased.
- `sender_name TEXT NULL` — display name portion of `From`.
- `recipients_to_json TEXT NULL` — JSON array of `{addr, name}` entries.
- `recipients_cc_json TEXT NULL`
- `subject TEXT NULL`
- `date_unix INTEGER NOT NULL` — message `Date` header as unix epoch seconds; falls back to first-seen timestamp if header is missing/invalid (and that fact is recorded in `last_error` of the originating sync state).
- `raw_path TEXT NOT NULL` — absolute path to the `.eml` file.
- `body_md TEXT NOT NULL` — Markdown body (mime-and-markdown slice produces this); empty string for body-less messages.
- `size_bytes INTEGER NOT NULL` — size of the raw `.eml` in bytes.
- `tombstoned_at INTEGER NULL` — unix epoch seconds when the server-side message was first observed missing.
- `UNIQUE (account_id, folder, server_uid, uidvalidity)`
- `FOREIGN KEY (account_id) REFERENCES accounts(account_id)`

Secondary indexes on `messages`:

- `(date_unix DESC)`
- `(thread_id, date_unix ASC)` — thread retrieval.
- `(sender_addr, date_unix DESC)` — `from` filter.
- `(folder, date_unix DESC)` — `folder` filter.
- `(account_id, date_unix DESC)` — account-scoped recent sweep.

`attachments` table:

- `attachment_id TEXT PRIMARY KEY` — `<message_id>:<index>`.
- `message_id TEXT NOT NULL`
- `filename TEXT NOT NULL`
- `mime_type TEXT NOT NULL`
- `size_bytes INTEGER NOT NULL`
- `FOREIGN KEY (message_id) REFERENCES messages(message_id)`
- Index `(message_id)`.

`index_queue` table (search-engine slice owns the row lifecycle; this slice owns the migration):

- `message_id TEXT PRIMARY KEY`
- `attempts INTEGER NOT NULL DEFAULT 0`
- `last_error TEXT NULL`
- `queued_at INTEGER NOT NULL`
- `failed_permanent INTEGER NOT NULL DEFAULT 0`
- Index `(failed_permanent, queued_at ASC)` for the drainer.

Connection topology:

- One write connection — used by IMAP sync worker and indexer task; serialized through a single tokio mutex.
- A read-pool of N read-only connections (N = small constant, e.g. 4) — used by search/message-fetch/thread handlers. Each opens with `?mode=ro`.
- Both hit the same WAL.

## §5 Sequence

1. **Daemon start.** Open `meta.sqlite` (write connection), `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`, `PRAGMA foreign_keys=ON`. Run migrations: read `schema_version.version`, apply each migration > current in order, bump `schema_version`. On any migration error, log `configuration parse error` and exit non-zero — daemon does not open the IPC socket. After migrations, reconcile `accounts` from `$XDG_CONFIG_HOME/scryd/config.toml` (Decision 12). Open the read-pool. Hand control to imap-sync, search-engine, and api slices.
2. **Message insert (sync worker).** IMAP sync slice fetches a message → mime-and-markdown slice produces `body_md` and attachment metadata → storage receives a message struct → BEGIN → `INSERT … ON CONFLICT … DO UPDATE` into `messages` (Decision 7) → `INSERT OR IGNORE` into `attachments` for each attachment metadata entry → write raw `.eml` bytes to `raw/<account>/<yyyy>/<mm>/<sha256-hex>.eml` mode `0600` (using a `.tmp` rename for atomicity) → run thread-resolution (Decision 8) and update `messages.thread_id` if it was minted as new → `INSERT OR IGNORE` into `index_queue` → COMMIT → notify the indexer task.
3. **Sync state update.** After processing a (account, folder) batch, imap-sync calls a storage update: `UPDATE sync_state SET last_seen_uid=?, last_full_sync_at=?, account_health=?, last_error=?, backoff_until=? WHERE account_id=? AND folder=?`.
4. **Tombstone.** When sync detects a UID disappeared (FETCH returned nothing for a UID we have): `UPDATE messages SET tombstoned_at=? WHERE message_id=?`. The indexer task's tombstone-remove handler is signaled; raw file is left on disk.
5. **Search read.** Search/api slice acquires a read connection from the pool → runs a SELECT joining `messages` against the Witchcraft-returned id list → applies filters (`from`, `since`, `until`, `folder`, `account`, `tombstoned_at IS NULL`) → returns rows → caller derives snippet from `body_md` per the search-engine slice.
6. **Message-fetch read.** api slice handler: `SELECT … FROM messages JOIN attachments WHERE message_id = ? AND tombstoned_at IS NULL`. Returns headers, body_md, attachment metadata.
7. **Thread-fetch read.** api slice handler: `SELECT … FROM messages WHERE thread_id = ? AND tombstoned_at IS NULL ORDER BY date_unix ASC`.
8. **Account inactivation.** Operator removes an account from config.toml → daemon next reconciles `accounts.active = 0` for that account_id → imap-sync stops scheduling that account → existing rows remain queryable for forensic recall (until the operator manually purges, which is out of scope for v1).
9. **Reindex (storage's part).** search-engine slice's reindex flow asks storage to enumerate every non-tombstoned `messages.message_id` and re-enqueue into `index_queue`. Storage runs `INSERT OR REPLACE INTO index_queue(message_id, attempts, last_error, queued_at, failed_permanent) SELECT message_id, 0, NULL, ?, 0 FROM messages WHERE tombstoned_at IS NULL`. Single statement; cheap.
10. **Crash recovery.** SIGKILL → restart → SQLite WAL replays automatically on first open → the next sync pass re-fetches against the same `last_seen_uid` and the `INSERT … ON CONFLICT … DO NOTHING / DO UPDATE` produces zero duplicate rows. Raw `.eml` writes use `.tmp` + rename, so a crash either left the old file in place or the new one fully written.

## §6 Out of scope

- The mime-and-markdown extraction logic that produces `body_md` and attachment entries (mime-and-markdown slice).
- IMAP fetch loop, IDLE handling, UID/UIDVALIDITY semantics, per-account scheduling (imap-sync slice).
- Witchcraft DB layout and witchcraft.sqlite migrations (search-engine slice; Witchcraft owns its own schema).
- HTTP request/response shapes (api slice).
- Snippet derivation (search-engine slice).
- Backup/restore tooling. Spec lists the data dir as a backup target; the storage slice ensures it's a coherent snapshot via single-file SQLite + flat raw store, and stops there.
- Manual-purge / vacuum operations (deferred to a future v2).
- Encryption at rest. Filesystem-permission isolation is the v1 boundary.

## §7 Open questions

- Whether `config.toml` should be re-reconciled on a SIGHUP or only at daemon start. Current decision: only at start (`add-account` already restarts via systemd or expects the operator to). Cli slice will confirm or amend.
- Whether `messages.thread_id` should be migrated when a later message arrives that bridges two previously-disjoint thread roots (a "thread merge"). v1 keeps the older `thread_id` and points the bridging message at it; the older root unifies. If thread merges in retro affect already-emitted threads, defer to v2.
- Whether `recipients_to_json` and `recipients_cc_json` should be normalized into a separate `recipients` table for indexable "messages-to/from-X" queries beyond the `from` filter the spec lists. Current decision: keep as JSON; spec only requires `from` as a filter.
- Maximum `body_md` size before we switch a message to file-store. Soft limit (e.g., 1 MB after Markdown extraction) above which we'd persist body to a file and store the path. v1 keeps everything inline; revisit if mailboxes routinely contain pathologically large messages.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
