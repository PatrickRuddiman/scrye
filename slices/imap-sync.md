Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — imap-sync

## §1 Summary

Owns talking to the IMAP server: connecting per account, asserting a strictly read-only command set, fetching messages by UID, holding IDLE channels and falling back to polling, tracking UID/UIDVALIDITY per (account, folder), recovering from disconnects, and isolating per-account failures so one account's outage never stalls another. Hands every fetched message to the storage and mime-and-markdown slices.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External Rust crates this slice leans on:

- `async-imap` (version pinned in Cargo.toml at coding time) — the only widely used async Rust IMAP client; supports IDLE, FETCH, SELECT/EXAMINE, CAPABILITY.
- `rustls` for TLS — pure-Rust, no system OpenSSL dependency, static-friendly. Aligns with the spec's posture (build-and-packaging slice will pin the TLS pick).
- `tokio` runtime — the daemon's async backbone (declared more concretely in build-and-packaging).

## §3 Decisions

1. **One supervisor task per account; one connection per (account, folder).** The daemon spawns an account-supervisor task per configured account; that supervisor spawns one connection task per folder in the account's `folders_json`. Rationale: IMAP allows one open mailbox per connection, so per-folder concurrency requires per-folder connections; per-account supervision gives us the failure-isolation boundary the spec promises ("a failure on one configured account does not stall sync or search for any other configured account").
2. **EXAMINE, never SELECT.** Folders are opened with `EXAMINE` (the read-only variant of `SELECT`). Rationale: makes the read-only invariant load-bearing at the protocol level; the server itself rejects mutating commands against an EXAMINE-opened mailbox.
3. **Mutating-command allow-list wrapper.** Wrap the `async-imap` client behind a thin send method that pattern-matches the outgoing command against an allow-list (`CAPABILITY`, `LOGIN`, `AUTHENTICATE`, `LOGOUT`, `EXAMINE`, `LIST`, `LSUB`, `STATUS`, `FETCH`, `UID FETCH`, `SEARCH`, `UID SEARCH`, `IDLE`, `DONE`, `NOOP`, `ID`, `ENABLE` for read-only extensions, `GETMETADATA`, `GETACL`, `GETQUOTA`, `GETQUOTAROOT`). Anything else: panic in debug, return an error and refuse to send in release. Rationale: backstop the EXAMINE guarantee with an in-process check; spec's acceptance criterion against a recording integration harness depends on no forbidden command leaving the daemon under any code path including error recovery.
4. **UID-keyed fetching.** All fetches use `UID FETCH`, never sequence numbers. Rationale: sequence numbers shift on insertions/deletions; UIDs are stable within a UIDVALIDITY epoch; spec promises restart-resumable progress that maps cleanly onto UID watermarks.
5. **Per-(account, folder) sync watermark.** Authoritative state lives in storage's `sync_state` table: `(uidvalidity, last_seen_uid)` per row. On a fetch pass, the connection issues `EXAMINE <folder>`, reads the response's UIDVALIDITY, and: if it differs from the stored `uidvalidity` (or stored is NULL), drop the watermark and re-sync from `UID 1` while keeping previously-stored `messages` rows for the prior UIDVALIDITY (they remain searchable; re-fetched copies land as new rows with the new UIDVALIDITY); otherwise, fetch `UID FETCH <last_seen_uid+1>:* …`.
6. **Initial-backfill chunk size.** 500 UIDs per `UID FETCH` request. Rationale: bounds memory and per-request wall time; commits to the watermark every 500 messages so a kill mid-backfill resumes within a small window.
7. **Body fetch shape.** `UID FETCH <range> (UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])`. Rationale: one round-trip per chunk; `BODY.PEEK[]` does not set the `\Seen` flag (read-only); we get headers and full body for raw `.eml` storage and the mime-and-markdown slice in one shot.
8. **IDLE preferred, poll fallback.** On connect, after LOGIN, run `CAPABILITY`. If `IDLE` is advertised, the connection task drops into IDLE after each fetch pass. If not, the task uses the operator-configured poll interval (default 300 s). Rationale: spec's `IDLE-to-queryable < 30 s` SLO depends on IDLE; not every server supports it; the fallback keeps the spec's polling-fallback freshness commitment honest.
9. **IDLE recycle window.** Re-issue `DONE` + new `IDLE` every 25 minutes. Rationale: most servers (including Gmail) close IDLE channels at 29 minutes (RFC 2177 recommends 29 minutes); 25 gives a safe margin.
10. **Fetch on IDLE notification.** When the IDLE channel reports an EXISTS / FETCH / RECENT untagged response, the task issues `DONE`, runs a `UID FETCH <last_seen_uid+1>:*`, processes new messages, advances the watermark, then re-enters IDLE. Rationale: EXISTS only tells us "there's something new"; we still have to fetch to know what.
11. **Per-account health and backoff.** Connection failures (connect refused, TLS failure, LOGIN rejected, server timeout, quota exceeded) update `sync_state.account_health` and `sync_state.last_error` for every folder of that account, set `backoff_until = now + min(2^attempts * 30s, 1h)`, and the account-supervisor sleeps until then before reconnecting. Rationale: bounded retry; per-account isolation; matches the spec's failure-mode acceptance criteria for unreachable / auth-rejected / quota-exceeded servers.
12. **Tombstone detection during incremental.** Each pass also runs `UID SEARCH ALL` periodically (every Nth pass; N = 10) to enumerate the server's current UIDs and compare against the local set; UIDs we have but the server doesn't get tombstoned via the storage slice's tombstone path. Rationale: cheap-ish; covers the "user deleted on another client" case without per-message HEAD requests; spec acceptance allows for some lag here (no specific SLO on detection of remote deletions).
13. **No cross-folder dedup at the IMAP layer.** A message that lives in INBOX and `[Gmail]/All Mail` is fetched twice — one row per (account, folder, server_uid). Rationale: simplest correct behavior; matches spec §5 open question (currently "treated as separate entries per folder"); thread reconstruction in storage already handles the second-row case via `header_message_id`-based thread joining.
14. **No SUBSCRIBE / UNSUBSCRIBE.** Folders to sync come from config only. We do not issue `LIST` to discover folders unless the operator explicitly enables an "auto-discover" config (out of scope for v1). Rationale: the operator's config is authoritative; auto-discovery is a v2 concern; less to test for read-only correctness.
15. **Connection-pool ceiling per account.** Maximum 5 concurrent connections per account (so up to 5 folders can sync in parallel; further folders queue). Rationale: many providers cap concurrent IMAP sessions per account (Gmail at 15, Outlook at 20, smaller hosts often lower); 5 is a conservative default that scales without provoking the cap.
16. **Internal cron / scheduler.** A long-lived "scheduler" task in this slice owns the per-account-supervisor lifecycles: it spawns supervisors at daemon start, watches them, restarts a supervisor that has died (with backoff), and re-reads the config-derived account list when storage signals "accounts reconciled" (after add-account). Rationale: spec's "internal scheduling" promise lives here; the operator never wires up a cron.

## §4 Contracts & shapes

Internal Rust crate (provisional name): `scryd-imap`.

Public surface:

- `Scheduler` — owns the lifetime of the per-account supervisor tree.
  - `Scheduler::new(storage_handle, config_handle, message_sink, log) -> Scheduler`
  - `Scheduler::start(&self) -> Result<(), SchedulerError>` — spawns supervisors per active account.
  - `Scheduler::reconcile(&self) -> Result<(), SchedulerError>` — re-reads accounts from storage, spawns new supervisors, signals removed-account supervisors to drain and exit.
  - `Scheduler::shutdown(&self) -> Result<(), SchedulerError>` — cooperatively shuts down all connections (`LOGOUT` then close).
- `MessageSink` — trait the storage slice implements; `imap-sync` writes through it.
  - `fn submit(&self, fetched: FetchedMessage) -> Result<(), StorageError>;`
  - `fn tombstone(&self, message_id: MessageId) -> Result<(), StorageError>;`
  - `fn update_sync_state(&self, account_id: &str, folder: &str, state: SyncStateUpdate) -> Result<(), StorageError>;`
- `FetchedMessage` — struct: `account_id`, `folder`, `server_uid`, `uidvalidity`, `internal_date`, `flags` (informational; never written back), `raw_bytes` (the BODY.PEEK[] payload). Mime parsing happens in mime-and-markdown slice, called from inside the storage `submit` flow — imap-sync doesn't parse.
- `SyncStateUpdate` — struct: `last_seen_uid`, `account_health`, `last_error`, `backoff_until`, `last_idle_at`, `last_full_sync_at` — all `Option<…>` so partial updates work.
- `CredentialFetcher` — trait the multi-instance-isolation / cli slice implements; reads the config file and returns the password-bearing credential **only on demand and only into a `Zeroize` buffer that's dropped after use**.

Allow-list types:

- `enum ImapVerb { Capability, Login, Authenticate, Logout, Examine, List, Lsub, Status, Fetch, UidFetch, Search, UidSearch, Idle, Done, Noop, Id, Enable, GetMetadata, GetAcl, GetQuota, GetQuotaRoot }` — the entire universe of verbs the wrapper will pass to `async-imap`.
- `fn assert_verb_allowed(v: &ImapVerb)` — `panic!` in debug, `Err(MutatingCommandRejected)` in release for any verb not in the enum (which is exhaustive by construction; the only way to hit the rejection is if a future code change adds a verb that was forgotten in the enum).

Per-connection state machine:

- `Disconnected` → `Resolving` → `Connecting` → `TlsHandshaking` → `LoggingIn` → `CapabilityChecking` → `Selecting (EXAMINE)` → `InitialBackfilling` (if first-time / UIDVALIDITY changed) → `Idling` (if IDLE supported) or `Polling` (otherwise) → on event/tick → `Fetching` → back to `Idling` / `Polling`. Errors at any state route to `Backoff` → `Disconnected`.

Connection limits:

- `MAX_CONNECTIONS_PER_ACCOUNT = 5` (Decision 15).
- `INITIAL_BACKOFF = 30 s`, `BACKOFF_CAP = 1 h`, exponential.
- `IDLE_RECYCLE = 25 minutes`.
- `INCREMENTAL_FETCH_BATCH = 500`.
- `TOMBSTONE_SCAN_EVERY = 10 incremental passes`.

## §5 Sequence

1. **Daemon start.** `scryd-imap::Scheduler::new` → `start` → for each `accounts.active = 1` row, spawn an account supervisor → for each folder in `accounts.folders_json`, supervisor spawns a connection task → connection runs the state machine through `Selecting (EXAMINE)`.
2. **First-time backfill on a folder.** Connection issues `EXAMINE <folder>` → reads UIDVALIDITY → storage's `sync_state.uidvalidity` is NULL or differs → connection enters `InitialBackfilling` → loops `UID FETCH 1:500`, `501:1000`, … each batch, hands each `FetchedMessage` to `MessageSink::submit`, after each batch updates the watermark via `update_sync_state` → on completion, `account_health=active`, transitions to `Idling` (if CAPABILITY had IDLE) or `Polling`.
3. **Incremental fetch (IDLE path).** Connection sits in `IDLE`. On EXISTS untagged response: issue `DONE` → `UID FETCH <last_seen_uid+1>:*` → submit each → update watermark → re-enter `IDLE`. Every `IDLE_RECYCLE` minutes, force a `DONE` + new `IDLE` even if no event arrived.
4. **Incremental fetch (poll path).** Connection sleeps for `poll_interval_seconds` → `EXAMINE <folder>` (server may have rolled UIDVALIDITY; check) → `UID FETCH <last_seen_uid+1>:*` → submit → update watermark.
5. **UIDVALIDITY change.** EXAMINE returns a different UIDVALIDITY → connection drops the watermark for that (account, folder) and runs the InitialBackfilling sequence again from UID 1. Existing `messages` rows from the prior epoch remain in storage and remain searchable; re-fetched copies land as new rows under the new UIDVALIDITY.
6. **Tombstone scan.** Every 10 incremental passes: `UID SEARCH ALL` → diff against `messages.server_uid` for that (account, folder, uidvalidity) → for each diff, `MessageSink::tombstone(message_id)`.
7. **Connection failure.** Any state-machine error routes to `Backoff` → log the appropriate failure category (`connect failure`, `tls failure`, `auth rejection`, `push-channel drop`, `uid-validity reset triggering re-sync` is not a failure — it's normal) → set `account_health` to the matching value via `update_sync_state` → sleep `backoff_until` → re-enter `Disconnected`.
8. **Account credential rotation.** Operator runs `add-account` for an existing account_id with a new password (cli slice). cli calls `Scheduler::reconcile` → for that account_id, supervisor closes existing connections (`LOGOUT`) and re-opens with the new credential. UIDVALIDITY is unchanged (same server, same mailbox), so `last_seen_uid` carries over and the next fetch is incremental.
9. **Account removed from config.** Reconcile → supervisor signals connections to `LOGOUT` and exit → `accounts.active = 0` → existing rows preserved.
10. **Daemon shutdown.** SIGTERM → `Scheduler::shutdown` → each connection sends `LOGOUT`, closes TLS, exits → supervisors exit → daemon process exits 0.

## §6 Out of scope

- MIME parsing of fetched bytes; HTML→Markdown conversion; attachment metadata extraction (mime-and-markdown slice). `FetchedMessage` carries raw bytes; storage's `submit` orchestrates the parse-then-write.
- Schema and writes to `meta.sqlite` and `raw/` (storage slice).
- Witchcraft indexing (search-engine slice).
- HTTP request/response shapes (api slice).
- The CLI's `add-account` interactive credential capture (cli slice).
- Per-arch build and TLS-stack pin (build-and-packaging slice).
- Log target wiring (observability slice). This slice declares which spec-closed-set failure categories it emits; observability owns the writes.
- OAuth / device-flow / refresh tokens. v1 is app-password only per the spec.

## §7 Open questions

- Whether to use `RFC 4549` (CONDSTORE / QRESYNC) for cheaper incremental sync when the server advertises it. Default decision: do not use in v1; keep the protocol surface small. Revisit if observed sync latency is unacceptable on huge mailboxes.
- Whether to honor `LIST-EXTENDED` to subscribe to a "MAILBOX MODIFIED" notification on the parent of the synced folders. Default decision: no, IDLE-per-folder covers the SLO.
- Whether `BODY.PEEK[]` should be replaced by `BODY.PEEK[HEADER]` + `BODY.PEEK[TEXT]` to allow earlier preview of headers before the full body is in. Default decision: no — we need raw bytes anyway for the spec's raw-message-bytes API.
- Behavior when a fetch returns a message that's already tombstoned in storage (server brought it back). Default: un-tombstone (clear `tombstoned_at`) and re-process; storage's `INSERT … ON CONFLICT … DO UPDATE` handles the row update.
- TLS verification stance against self-signed corporate IMAP servers. Default: require valid TLS via the system root store; provide no per-account "skip verification" option. Operator with a private CA must add it to the system trust store. Confirm this matches the spec's read-only-and-secure posture.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
