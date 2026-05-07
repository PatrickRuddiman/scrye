# scryd — Read-Only IMAP Mail Indexer & Search Daemon

## §1 Summary

On a Linux host, each user (the operator) installs their own instance of the daemon. The instance holds that operator's IMAP app password, pulls that operator's mail, and exposes a search service to processes running as the same operator (callers — that operator's scripts, AI agents, CLI tools) on the local machine. Multiple operators can run their own instances on the same host; each instance is owned by exactly one local user, and OS-level identity prevents any process running as a different local user from reaching that instance's API or reading any of its data. The IMAP server is treated strictly as read-only — scryd never modifies server-side state. Without scryd, callers either get the credential themselves or have no way to search mail at all; with scryd, an operator's callers get rich mail search and content retrieval over their operator's instance, and they never touch the credential.

Top-level promises, each measurable end-to-end:

- An instance's IMAP credential is readable on disk only by that instance's owning user. No other non-superuser local user can read it.
- A process running as user A cannot search, retrieve, list, or otherwise observe any mail, accounts, results, snippets, raw bytes, or sync state held by an instance owned by user B, regardless of how it tries (no API call, no read of a data file, no IPC channel). The boundary is enforced by the operating system, not by application-level credential checks.
- Multiple instances can coexist on the same host, each owned by a different local user, each syncing and serving search independently. One user installing, starting, stopping, restarting, reindexing, or breaking their instance has no observable effect on any other user's instance.
- When an operator runs the CLI, it acts on that operator's own instance only. It does not require configuration to find that instance and cannot be pointed at another operator's instance.
- Search is available continuously from the moment an instance starts — including during the first-run full backfill, against a partially-built or empty index — without errors to the caller.
- Full-text search returns ranked, snippeted results in p95 < 200 ms on a warm index.
- Hybrid search (full-text + semantic) returns a single merged ranking that includes results unreachable by full-text alone (paraphrase / synonym matches).
- New mail delivered while the instance holds an active server-push channel becomes searchable in p95 < 30 seconds from server notification.
- New mail delivered while the instance is in polling fallback becomes searchable within the operator-configured poll interval plus 30 seconds.
- Across an instance restart in the middle of a sync, the resulting metadata and index contain exactly one entry per server-side message — no duplicates, no missing messages relative to the same run-to-completion.
- The daemon never issues an IMAP command that mutates server state (no STORE-flag, no COPY, no MOVE, no EXPUNGE, no APPEND, no DELETE), under any code path including error recovery.
- A failure on one configured account (auth rejected, server unreachable, TLS handshake failure, quota exceeded, mailbox UIDVALIDITY reset) does not stall sync or search for any other configured account in the same instance.
- The CLI surface visible to a human operator is exactly three verbs: add an account, run a full index, run a search.
- Sync scheduling is internal to the instance. The operator does not configure, install, or maintain any external timer, cron job, or scheduler entry. Once the instance is started, first-run full backfill and recurring incremental syncs happen on the instance's own schedule with no further operator action.

## §2 Behavior

### Personas

- **Operator** — a local Linux user who installs and owns one instance of the daemon for themselves. The operator configures their accounts, triggers full reindexes on their instance, runs ad-hoc searches against their instance, and reads system logs scoped to their instance. Multiple operators on a single host each own their own separate instance; there is no "super-operator" that manages all instances. There is no separate "monitor" or "admin" persona; each operator does all operator tasks for their own instance only.
- **Caller** — a local process (script, CLI tool, AI agent, or other service) running as the same local user as the instance it calls, on the same machine. Callers do not present credentials at the API; the operating system's process identity is the only identity check. Every operation the instance's API exposes is callable by callers running as the owning user. Processes running as a different local user are not callers and cannot reach the instance.
- **Superuser (out of scope)** — `root` and equivalently privileged identities are outside the trust model. They can observe any instance's data and the spec does not attempt to defend against them. Operator-vs-operator isolation is the in-scope boundary.

### Operator's recurring tasks and lifecycle transitions

- Install scryd as a per-user instance on a Linux host that may already host other operators' instances, without touching their data or interrupting their service.
- Bring my own instance under a per-user service supervisor that starts it on login or boot and restarts it on failure.
- Configure one or more accounts on my own instance (host, port, username, app password, folders to sync).
- Replace a credential for an existing account on my own instance (e.g., after an app-password rotation on the provider side).
- Trigger a full reindex on my own instance (rebuild the search artifacts from previously fetched message data).
- Run an ad-hoc search from the CLI; the CLI talks to my own instance only.
- Read system logs scoped to my own instance to determine whether a given account is healthy, when it last synced, and what failed if anything failed. Logs from other operators' instances are not part of my view.
- Uninstall my own instance (stop, remove its data, remove its config) without affecting any other operator's instance on the same host.

### Caller's recurring tasks

- Submit a search query, optionally filtered by sender, date-since, date-until, folder, and result-limit, in one of three modes: full-text, semantic, hybrid.
- Retrieve a parsed message by its identifier (headers, body in Markdown, attachment metadata).
- Retrieve the original raw bytes of a message by its identifier.
- Retrieve a full thread, oldest message first, by thread identifier.
- List configured accounts (account identifiers and their configured folder sets — no credentials).
- Trigger an immediate sync.

### User stories

- As an operator, I want to add an IMAP account by entering host, username, and app password once at the CLI, so that the daemon can begin syncing without me writing the credential anywhere a script can read it.
- As an operator, I want to update the app password for an account that already exists, so that a provider-side rotation does not require me to re-download all mail or rebuild the index.
- As an operator, I want to trigger a full reindex from the CLI, so that I can recover after corrupting the search artifacts or after changing indexing settings, without losing the locally cached message data.
- As an operator, I want to run a search query from the same CLI, so that I can verify the daemon is returning results without writing a separate client.
- As an operator, I want all health, sync state, and failures to appear in the system log stream, so that I can diagnose and alert with the same tools I use for any other service on the machine.
- As an operator on a multi-user host, I want to install my own scryd instance for my own mail without disturbing any other user's existing instance, so that we can each have personal mail search on the same machine.
- As an operator, I want every other non-superuser local user on the host to be unable to read my mail, my credential, my search results, or even discover that I have an instance, so that mail privacy on a shared host is enforced by the OS and not by trust between me and other users.
- As an operator, I want my CLI invocations to act on my own instance automatically — no port number to remember, no socket path to configure, no risk of accidentally pointing at another user's instance — so that the tool is safe to use on a shared host.
- As a caller, I want to submit a search query in full-text mode and get ranked results with snippets, so that I can find a known phrase quickly.
- As a caller, I want to submit a search query in semantic mode and get results matched by meaning, so that I can find mail when I don't remember the exact words.
- As a caller, I want to submit a search query in hybrid mode and get a single merged ranking, so that I get the strengths of both modes without orchestrating two queries.
- As a caller, I want search to succeed and return whatever results are available right now — including the empty list — even if the daemon is still in its first-run backfill, so that my downstream logic does not need a "wait for ready" branch.
- As a caller, I want to retrieve a parsed message by identifier with the body rendered as Markdown, so that I can feed it to a language model or display it without doing my own MIME parsing.
- As a caller, I want to retrieve a thread by identifier, so that I can show or process the full conversation context.
- As a caller, I want to list configured accounts, so that I can scope a search to a specific account if I'm working on behalf of a multi-account operator.
- As a caller, I want to trigger an immediate sync, so that I can guarantee freshness before a downstream task runs.
- As a caller, I want to retrieve the raw bytes of a message, so that I can preserve evidence, run my own MIME parser, or forward the original to another tool.

### Acceptance criteria

- Given the daemon is installed and one account is configured but no mail has yet been fetched, When a caller submits a search for "invoice" in full-text mode, Then the response is a successful, well-formed result list (which may be empty) — not an error and not a "not ready" status.
- Given the daemon is mid-way through its first-run backfill of an account containing 50,000 messages, When a caller submits a search for "invoice" in full-text mode, Then the response is a ranked result list reflecting the messages indexed so far, returned in under 200 ms p95 on a warm process.
- Given the daemon has finished a first-run backfill of an account, When a caller submits a search for "invoice" in full-text mode with limit 5, Then the response contains up to five ranked hits, each carrying a snippet of the message body in Markdown showing the matched terms in context, plus the message identifier, sender, date, folder, and account.
- Given the operator has configured semantic indexing and the daemon has finished embedding the corpus, When a caller submits a query "questions about onboarding new hires" in semantic mode, Then the response contains messages whose meaning matches the query even when none of those exact words appear in any matching message.
- Given full-text and semantic indexes are both populated, When a caller submits a query in hybrid mode, Then the response is a single ranking that contains messages found by either signal, ordered such that messages found by both signals rank no lower than messages found by only one.
- Given a caller submits a search with all of {q, from, since, until, folder, limit, mode} set, When the request is processed, Then the response contains only messages matching every supplied filter and is at most the requested limit.
- Given a caller retrieves a message by identifier, When the response is returned, Then it contains the parsed headers (subject, from, to, cc, date, folder, message-id, in-reply-to, references, thread-id), the body rendered as Markdown, and an attachment list where each attachment has filename, MIME type, and size in bytes — but no attachment payloads.
- Given a caller retrieves a thread by identifier, When the response is returned, Then it contains every message scryd has indexed for that thread, ordered from oldest to newest by message date.
- Given a caller submits a request to retrieve raw message bytes by identifier, When the message exists in the local store, Then the response contains the original message bytes exactly as they were fetched from the IMAP server, with no additional authentication required.
- Given a caller triggers an immediate sync, When the request is processed, Then the daemon performs a sync pass against every healthy account and the request returns once the pass has begun, without blocking other in-flight searches and without requiring authentication.
- Given the operator runs the full-reindex CLI verb, When the rebuild is in progress, Then search requests continue to be served and return whatever portion of the corpus has been (re-)indexed at query time, the same way they do during the first-run backfill — there is no "rebuild in progress" error returned to any caller.
- Given the operator runs the add-account CLI verb with a new account identifier and provides host, username, and app password, When the verb completes successfully, Then the daemon (if running) begins backfilling that account in parallel with any existing accounts, without re-downloading or re-indexing any other account, and other accounts' searches remain available throughout.
- Given the operator runs the add-account CLI verb with an existing account identifier and provides a new app password for the same host and username, When the verb completes successfully, Then the daemon resumes sync against that account using the new password without losing per-folder UID-validity state and without forcing a full re-download or full reindex.
- Given the operator runs the full-reindex CLI verb, When the verb completes, Then the search artifacts contain exactly one entry per locally cached message, the count matches the locally cached message count, and search continued to be served throughout.
- Given the operator runs the search CLI verb against the local daemon, When results are returned, Then the output contains the same fields a programmatic caller would receive over the local API.
- Given a server has notified the daemon of a new message via its push channel, When the daemon receives the notification, Then the new message is fetched, parsed, indexed, and queryable in p95 < 30 seconds from the notification.
- Given the daemon is in polling fallback against an account, When new mail arrives on the server between two polls, Then the new mail is queryable within the configured poll interval plus 30 seconds of the server-side delivery time.
- Given an integration harness records every IMAP command issued by the daemon over a session including the daemon's own restart and recovery paths, When the session ends, Then no recorded command is one that mutates server-side state (no STORE-flag, no COPY, no MOVE, no EXPUNGE, no APPEND, no DELETE).
- Given the daemon is killed (SIGKILL) while a sync is in progress and is then restarted, When the next sync pass for the affected account completes, Then the locally cached messages and the search artifacts contain exactly one entry per server-side message — no duplicates and no gaps relative to a clean run-to-completion.
- Given two operators (alice, bob) each have a running scryd instance on the same Linux host, When a process running as alice attempts to connect to bob's instance API by any means (the bob-instance's listening endpoint, a known socket path, an enumerated port), Then the attempt is rejected before any application-level request body is processed, no message, account, snippet, or sync-state field of bob's reaches alice's process, and an entry is recorded in bob's instance log identifying that a connection from a non-owning user identity was rejected.
- Given alice and bob each have a running scryd instance on the same host, When alice runs the search CLI verb with the query "invoice", Then the results contain only mail from alice's own configured accounts, no field of any result references bob's accounts or mail, and the result set is byte-identical to the result alice would have received if bob's instance were not running at all.
- Given alice and bob each have a scryd instance on the same host, When a process running as alice issues a read against the on-disk file containing bob's IMAP credential, Then the read fails with a permissions error and no bytes of the credential are returned to alice's process.
- Given a process running as alice attempts to discover whether bob has a scryd instance by listing bob's data directory, listing bob's configuration directory, or probing endpoints, When the probes are issued, Then alice receives only OS-level "not found" or "permission denied" errors equivalent to what she would receive if bob's instance did not exist; no field returned to alice distinguishes "bob has an instance you can't access" from "bob has no instance".
- Given the operator runs the CLI verb without specifying any instance, target, port, or socket, When the CLI executes, Then it acts on the operator's own instance, succeeds if that instance is running, and fails with a clear "your instance is not running" message if it is not — and in no case acts on, or returns data from, any other user's instance.

### Failure modes

- Given the IMAP server for an account is unreachable, When the daemon retries on a backoff schedule, Then that account is logged as degraded with the reason and the timestamp of its last successful sync, search continues to serve the previously indexed mail for that account, and other accounts continue to sync and serve unaffected.
- Given the IMAP server rejects the credentials for an account, When the rejection is observed, Then that account stops attempting to sync until the operator updates the credential, the rejection is logged with the account identifier, search continues to serve the previously indexed mail for that account, and other accounts continue to sync and serve unaffected.
- Given the IMAP server reports a UID-validity reset on a folder (e.g., the mailbox was rebuilt server-side), When the reset is observed, Then the daemon discards its prior UID state for that folder and re-syncs the folder from the start, and search results for that folder remain available against the previously indexed mail until the re-sync overwrites them.
- Given the server's push channel drops, When the drop is observed, Then the daemon reconnects with backoff and falls back to polling for that account in the meantime; search remains available throughout.
- Given a single message fails to parse (malformed MIME, encoding error, truncated payload), When the parse fails, Then the failure is logged with the account, folder, and server-side identifier, that message is skipped, and sync of the account continues with the next message.
- Given the semantic indexer fails on a message, When the failure occurs, Then the message is still indexed in full-text and is still available for retrieval; the semantic-side failure is logged; and sync continues.
- Given the full-text indexer fails on a message, When the failure occurs, Then the message is still available for retrieval and (if applicable) for semantic search; the full-text-side failure is logged; and sync continues.
- Given the local data directory has run out of space, When a write fails, Then ingestion stops cleanly, the disk-full condition is logged, search continues to serve the existing index, and ingestion resumes automatically once space is available.
- Given the configuration file is malformed at daemon start, When the daemon attempts to start, Then the daemon refuses to start, logs the parse error with line number, and the supervisor reports the failure.
- Given the configuration file's permissions are not restricted to the instance's owning user (e.g., world-readable, or owned by a different user than the running instance), When the instance attempts to start, Then the instance refuses to start and logs the permission problem so the operator sees it on the next supervisor status check.
- Given a process running as a different local user than the instance's owning user attempts to connect to the instance API, When the connection attempt is made, Then the instance refuses to serve the request before any application-level command is dispatched, the request returns a permission-style error, and an entry is recorded in the owning user's instance log.

## §3 Scope

**In:**

- Continuous, idempotent, read-only sync from one or more configured IMAP accounts, with first-run full backfill triggered automatically.
- Per-account isolation within an instance: failures on one account do not affect any other account's sync or search in the same instance.
- Multi-instance coexistence on a single Linux host: each user installs their own instance and runs it as their own user. Multiple instances on one host operate independently.
- Per-user isolation across instances: a process running as user A cannot reach, observe, or affect any data, API, log entry, or runtime state belonging to an instance owned by user B. The boundary is enforced by OS-level identity, not by application-level credential checks.
- Per-user CLI auto-routing: when an operator runs the CLI on a machine, it acts on that operator's own instance with no configuration and cannot be aimed at another operator's instance.
- Full-text, semantic, and hybrid search modes over the corpus, with filters for sender, date-since, date-until, folder, and result-limit.
- Snippets in returned search hits.
- Retrieval of any indexed message as parsed headers + Markdown body + attachment metadata (no payloads).
- Retrieval of a full thread (oldest first).
- Retrieval of the original raw bytes of a message.
- Listing of configured accounts (no secrets).
- Trigger for immediate sync (API).
- Operator-triggered full reindex (CLI only), during which search continues to serve against whatever portion of the corpus is currently indexed.
- A local-only API with no per-caller credential check: every operation is callable by any process running as the instance's owning user; the operating system is the only access control.
- A CLI surface containing exactly three verbs visible to the operator: add an account (which doubles as credential rotation when run with an existing identifier), trigger a full reindex, run a search.
- All health, sync state, and error reporting via the system log stream, scoped to the owning operator's view of the system log.

**Out:**

- Sending, drafting, replying, forwarding, deleting, moving, flagging, or marking-read of mail. The daemon is read-only against the IMAP server.
- A web UI, a TUI, a desktop app, or any GUI surface.
- A query-able health endpoint exposed to callers. Callers learn the state of the world by issuing search/retrieval requests; operators learn it from system logs.
- A status, list-accounts (CLI), or rotate-password (CLI) verb. The operator's CLI is restricted to the three verbs listed in §3 In.
- Calendar, contacts, or task sync.
- OAuth flows (device flow, web flow, refresh tokens). v1 authenticates with app passwords only and will fail closed against any provider that disallows them.
- Cross-account deduplication of messages that appear in multiple accounts.
- Saved-query webhooks or any push notification of new matches.
- Any rule engine that tags, classifies, or otherwise modifies the indexed view of a message.
- Network-exposed access. The instance's API is reachable from the local machine only.
- Application-layer per-caller credentials. The API has no token, no Authorization header, no API key, and no per-caller credential check. Caller identity is whatever the operating system reports about the connecting process; that identity is the only access control beyond "same machine, same user".
- A single shared instance serving multiple operators' mail. Each operator runs their own instance.
- Cross-operator search, listing, discovery, or aggregation. An instance only ever sees its owning operator's mail.
- A reindex API operation. Reindex is an operator action only, available exclusively through the CLI; no caller (script, agent, or other process) can trigger a reindex over the API.
- Defense against the superuser (`root` or equivalent). The spec does not promise that root cannot read another operator's data.
- Attachment payload retrieval (only metadata is returned).
- Attachment text extraction (PDF, docx, etc.) into the search index.

## §4 Quality bars

### Performance

- Full-text search: p95 first-result latency < 200 ms on a warm process against a corpus of 50,000 messages.
- Semantic search and hybrid search: must return successfully on the same warm process; no specific latency bound is committed in v1.
- IDLE-to-queryable freshness: from the moment the IMAP server notifies the daemon of a new message to the moment that message appears in a full-text search query result, p95 < 30 seconds.
- Polling-fallback freshness: from the moment a new message becomes deliverable on the server to the moment it appears in a full-text search query result, the operator-configured poll interval plus 30 seconds at p95.
- Search availability during initial backfill, during incremental sync, and during a reindex: search requests succeed (well-formed response, possibly empty) at all times the daemon process is running.

### Reliability

- A daemon restart at any point during a sync, including the most recent message in flight, leaves the daemon's local state recoverable on the next sync pass with no duplicate entries and no missing entries relative to a clean run-to-completion.
- A failure on one account does not stall, slow, or otherwise affect the sync or search behavior of any other account, as observed by callers and by the latency targets above.

### Security and privacy

- An instance is owned by exactly one local user (the operator who installed it). Each instance holds exactly one operator's credentials, locally cached messages, and search artifacts.
- The IMAP app password is readable on disk only by that instance's owning user. No other non-superuser local user can read the credential — including unprivileged users, other operators on the same host, the user that ran the installer if different from the owning user, and any caller process.
- The instance's API is reachable from the local machine only and reachable only by processes whose operating-system identity is the instance's owning user. Connections from any other host on the network are unreachable; connections from local processes running as any other local user (excluding the superuser) are rejected before any application-level command is dispatched. There is no API token, no Authorization header, no API key — process identity reported by the operating system is the only access control.
- The complete set of identities allowed to call an instance's API is: processes running as that instance's owning user, on the same host as the instance. Processes running as any other non-superuser local user, and any process on any other host, are rejected.
- A process running as user A cannot read, search, retrieve, list, snippet, observe, or in any other way obtain content of any message held by an instance owned by user B, on the same host. This applies to the API, to direct reads of any on-disk file the instance maintains, and to any IPC channel the instance exposes.
- The set of fields any instance returns to non-owning callers — over any channel — is empty. There is no behavior, no error message, and no observable side-effect from which a non-owning caller can derive that user B has accounts named X or that user B has indexed N messages.
- The daemon never issues an IMAP command that mutates server state. The complete set of forbidden command classes is: STORE-flag (any flag-write, including marking-read), COPY, MOVE, EXPUNGE, APPEND, DELETE, RENAME, SUBSCRIBE, UNSUBSCRIBE, SETMETADATA, SETACL, SETQUOTA, and any extension command that has equivalent effect. Read-only commands (SELECT/EXAMINE in read-only mode, FETCH, SEARCH, IDLE, NOOP, CAPABILITY, LIST, LSUB-as-read, GETMETADATA, GETACL, GETQUOTA, ID, ENABLE for read-only extensions, AUTHENTICATE, LOGIN, LOGOUT) are permitted.
- The complete set of operations the API exposes is: submit a search query, retrieve a parsed message, retrieve raw message bytes, retrieve a thread, list configured accounts, trigger an immediate sync. Every one of these is callable, without application-level authentication, by callers running as the instance's owning user. None of them are callable by any other identity. Full reindex is not in this set; it is only invocable through the operator's CLI.
- The complete set of fields returned in a list-accounts response is: account identifier, configured folder set. No credential material, no auth state, no per-account error or sync timestamp.
- The complete set of fields returned in a parsed-message response is: subject, from, to, cc, date, folder, message-id, in-reply-to, references, thread-id, body (Markdown), attachment list (each entry: filename, MIME type, size in bytes). No attachment payloads.
- The complete set of filters accepted on a search query is: query string, sender, date-since, date-until, folder, result-limit, mode. The complete set of accepted modes is: full-text, semantic, hybrid.
- The complete set of failure categories surfaced in the system log stream is: connect failure, TLS failure, auth rejection, push-channel drop, UID-validity reset triggering re-sync, single-message parse failure, single-message full-text indexer failure, single-message semantic indexer failure, disk-full, configuration parse error, configuration permission error, non-owner-user connection rejection.
- The superuser (`root` or equivalent) is outside the trust model. The spec does not promise isolation against root.

### Compatibility

- The daemon must run on Linux. Each instance runs as the local user that installed it.
- Multiple instances must be able to coexist on a single Linux host with no shared state, no shared listening endpoint, and no shared on-disk artifacts. Installing, starting, stopping, or removing one instance must have no observable effect on any other instance running on the same host.
- The supervisor model is per-user (each operator's instance runs under a per-user service supervisor that the operator controls); there is no requirement for a system-wide service unit and no requirement that the operator have any system-wide privileges.

## §5 Open questions

- IDLE-to-queryable freshness target is set to p95 < 30 seconds. If the operator's expectation is sub-10-second freshness or sub-minute is fine, confirm.
- The hybrid mode promises a single merged ranking that ranks dual-signal hits no lower than single-signal hits. If a stricter ranking guarantee (e.g., reciprocal-rank fusion specifically, or a learned blend) is required, specify.
- "Trigger an immediate sync" via the API: confirm whether the response should return after the pass has *begun* (asynchronous) or after the pass has *completed* (synchronous). The current spec says it returns once the pass has begun.
- Cross-account searches are assumed to span all configured accounts within the calling operator's instance when the account filter is absent. Confirm this is desired vs. requiring an explicit account in every query.
- A message that exists in multiple folders within the same account (e.g., Gmail's "All Mail" plus a label folder) is currently treated as separate entries per folder. Confirm this is desired or whether the daemon should canonicalize per-account.
- Confirm the multi-instance install path: does scryd ship a single binary that an operator places anywhere they want, with all per-user data living under their home directory or per-user runtime directory — or is a system-wide binary with per-user data still acceptable? The spec presumes per-user data; a system-wide binary is compatible with that.
- Two operators on the same host running different versions of the binary: is this expected to work (each operator manages their own version), or must all instances on a host be the same version? The spec is currently silent.

> If any requirement is ambiguous, stop and ask. Before producing a plan, list your assumptions and the implementation choices you intend to make. Do not write code until those are confirmed.
