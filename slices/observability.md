Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — observability

## §1 Summary

Owns how the daemon emits the system-log stream the spec promises is the only health/state surface: log target, format, severity mapping, the concrete shape of every entry in the spec's closed set of failure categories, and the redaction rules that keep the IMAP credential out of every log line. Every other slice declares which categories it emits; this slice owns the bytes that actually leave the process.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External pieces this slice leans on:

- `tracing` + `tracing-subscriber` — structured logging crate with span/event support; the de-facto Rust choice; pinned in build-and-packaging's `[workspace.dependencies]`.
- `systemd-journald` — captures stderr from any `Type=simple` user-level service automatically; no file path management, no rotation, `journalctl --user -u scryd` gives the operator filtering for free.

## §3 Decisions

1. **Target = stderr → journald.** Daemon writes log lines to stderr; systemd captures stderr into the user's journal because the unit (build-and-packaging slice) is `Type=simple`. No file paths, no logrotate, no syslog socket. Rationale: spec says "log to files or dmesg or however daemons log"; journald is the systemd-native answer; per-user journal scoping comes for free (Decision 5).
2. **Format = JSON-Lines.** Each event is one line of JSON written to stderr. When stderr is a TTY (developer running `cargo run`), the subscriber switches to a human-readable pretty format instead. Rationale: structured fields survive `journalctl --output=cat | jq …`; human format keeps developer ergonomics; the daemon detects via `isatty(stderr)` at startup and never changes mid-run.
3. **One subscriber, configured at process start.** `tracing_subscriber::registry()` with a JSON layer (or pretty layer for TTY) and an `EnvFilter` reading `RUST_LOG` (default `scryd=info,scryd_api=info,scryd_imap=info,scryd_search=info,scryd_storage=info,scryd_mime=info,scryd_runtime=info,warn`). Rationale: `RUST_LOG` is the universal Rust verbosity dial; the default lets the spec's failure-category lines through and quiets per-message DEBUG noise.
4. **Closed event field set.** Every event carries a subset of: `ts` (RFC 3339, UTC), `level` (`error`|`warn`|`info`|`debug`|`trace`), `target` (the source module path — `tracing`'s built-in), `category` (the spec's closed-set string when this is a failure-category event; absent otherwise), `account_id` (string), `folder` (string), `message_id` (string), `peer_uid` (i32), `peer_pid` (i32), `error` (string — already-formatted error text), `subtype` (string — for parse/indexer faults that have variants), `account_health` (string — when the event reflects a health transition), `kind` (string — short discriminator for non-failure events: `startup`, `shutdown`, `sync_pass_start`, `sync_pass_complete`, `idle_enter`, `idle_drop`, `idle_recycle`, `backfill_progress`, `reindex_start`, `reindex_complete`, `account_reconciled`, `request`, `response`). Fields absent from a given event are omitted entirely. Rationale: predictable shape for downstream log tools; `category` is the closed-set field operators filter by; absent fields keep lines short.
5. **Per-instance scoping is the kernel's job, not the daemon's.** A user's `systemctl --user` instance writes to that user's journal, period. `journalctl --user -u scryd` on alice's session shows only alice's logs. The daemon does not stamp a `user` field — that would be redundant and could leak across operators if logs were ever aggregated centrally. Rationale: spec's "logs from other operators' instances are not part of my view"; the OS already enforces this.
6. **Severity map.**
    - **ERROR**: hard failures the operator must see — `auth rejection` (after retry budget exhausted), `disk-full`, `configuration parse error`, `configuration permission error`, `non-owner-user connection rejection`. Also: any 5xx response from the api layer.
    - **WARN**: transient or per-message — `connect failure`, `TLS failure`, `push-channel drop`, `single-message parse failure`, `single-message full-text indexer failure`, `single-message semantic indexer failure`. Also: 4xx api responses.
    - **INFO**: lifecycle events — startup, shutdown, account-reconciled, sync-pass-start/complete per account, IDLE enter/drop/recycle, reindex-start/complete, backfill-progress at coarse milestones (every 1000 messages or every 60 s).
    - **DEBUG**: per-message progress, per-request access lines (one per API request).
    - **TRACE**: IMAP wire commands (off by default; for hand-debugging only).
   `UID-validity reset triggering re-sync` is **INFO** with `kind=uidvalidity_reset` — it's a routine recovery event, not a failure; it is enumerated in the spec's failure-category list because operators want it surfaced, but the severity is informational since the daemon handles it transparently.
7. **Credential redaction.** The daemon never accesses the IMAP password except through the `CredentialFetcher` trait (imap-sync slice §4) which returns a `Zeroize` buffer; that buffer is `Drop`-zeroed on scope exit and is **never** placed into a tracing field. To make this enforceable in code review, the `scryd-config` crate's password type wraps a `secrecy::SecretString` that does not implement `Debug`/`Display` for its contents — only `[REDACTED]`. Rationale: the credential is the spec's hardest privacy promise; a logging-side leak would void it; making the type unprintable kills the most common accidental-leak vector at compile time.
8. **Subject and body redaction.** Subjects and bodies are message *content*, not credentials, but they're sensitive. Default: subjects appear in `INFO` lifecycle events ONLY when truncated to 64 chars (so log lines stay scannable) and never appear in `ERROR`/`WARN` lines about that message — those use only `account_id`, `folder`, `message_id`. Body bytes never appear in any log entry, period. Rationale: log lines are the operator's debugging surface; full subjects are sometimes useful for triage; bodies are not.
9. **Startup banner.** Exactly one INFO event at process start, `kind=startup`, fields: `version` (cargo pkg version), `commit` (env `SCRYD_COMMIT_HASH` if set at build time, else absent), `arch`, `witchcraft_features` (the feature-flag list that was actually built in), `accounts` (count), `assets_path`, `data_dir`, `socket_path`, `idle_supported_default` (read from config). Rationale: a single line answers "which build, with which features, against which paths, with how many accounts" — the most common first-question on every triage.
10. **Shutdown event.** One INFO event on graceful shutdown (`kind=shutdown`, fields: `reason` ∈ {`sigterm`,`sigint`,`error`}, `uptime_seconds`, `clean_close` boolean). Rationale: closes the loop on lifecycle visibility.
11. **API access log = DEBUG, not INFO.** Each successful request gets one DEBUG event (`kind=request`, fields: `method`, `path`, `status`, `duration_ms`). Each non-2xx gets one WARN or ERROR per Decision 6. Rationale: search-heavy callers (chat agents) can fire dozens of requests per turn; INFO-level access logs would flood the journal; the spec doesn't require access logging at all.
12. **No metrics endpoint in v1.** No `/metrics` route, no Prometheus exporter, no OpenTelemetry. Logs only. Rationale: spec is explicit that there's no health endpoint; same posture for metrics. v2 may revisit.
13. **No client-IP / connection-source field on api access lines.** Always loopback, always the operator's uid (multi-instance-isolation enforces). No useful info to log; would just bloat lines. Rationale: avoid recording fields that carry no decision-making value for the operator.
14. **Log volume floor: no in-process rate limiting.** Journald has its own per-service rate limit (`RateLimitIntervalSec`/`RateLimitBurst`); when journald drops, it emits a "missed N messages" entry of its own — that's the operator's signal. The daemon will not silently drop log entries. Rationale: simplest correct behavior; the rate-limit concern belongs to the system, not the application.
15. **`tracing` macro discipline.** Categorized failures are emitted with explicit `tracing::error!`/`warn!` calls that include the literal `category` field as a string — never via `Display` of an opaque error type. Rationale: lint-able by `clippy`-style review for the category-string allowlist; harder for a refactor to drift the category names away from the spec's closed set.

## §4 Contracts & shapes

Internal Rust crate (provisional): `scryd-log`. Pulled in by every other crate.

Initialization function:

- `init() -> Result<(), InitError>` — called once near the top of `scryd-runtime::serve`. Reads `RUST_LOG`, decides JSON vs pretty based on `isatty(stderr)`, installs the global subscriber.

Helper macros (thin wrappers around `tracing::event!`) the slices' code uses to emit categorized failures:

- `log_failure!(category = "<closed-set string>", error = ?<err>, …other_fields)`
- `log_lifecycle!(kind = "<discriminator>", …other_fields)`

These ensure every categorized failure carries the literal category string and the spec-defined fields. Other slices import the macro from `scryd-log`.

Closed `category` value set (matches the spec's §4 Security failure-category list verbatim):

- `connect failure`
- `tls failure`
- `auth rejection`
- `push-channel drop`
- `uid-validity reset triggering re-sync`
- `single-message parse failure`
- `single-message full-text indexer failure`
- `single-message semantic indexer failure`
- `disk-full`
- `configuration parse error`
- `configuration permission error`
- `non-owner-user connection rejection`

Closed `kind` value set for non-failure lifecycle events:

- `startup`, `shutdown`
- `sync_pass_start`, `sync_pass_complete`, `backfill_progress`
- `idle_enter`, `idle_drop`, `idle_recycle`, `uidvalidity_reset`
- `reindex_start`, `reindex_complete`
- `account_reconciled`
- `request` (api access log)

Example event lines (intent, not literal payloads):

- IMAP auth rejection (ERROR):
  ```
  {"ts":"2026-05-07T11:32:04.512Z","level":"error","target":"scryd_imap::auth","category":"auth rejection","account_id":"primary","folder":"INBOX","error":"AUTHENTICATIONFAILED"}
  ```
- Single message parse failure (WARN):
  ```
  {"ts":"2026-05-07T11:32:04.601Z","level":"warn","target":"scryd_mime::parse","category":"single-message parse failure","subtype":"body-truncated","account_id":"primary","folder":"INBOX","message_id":"primary:abc123@…"}
  ```
- Non-owner-user connection rejection (ERROR):
  ```
  {"ts":"2026-05-07T11:32:04.700Z","level":"error","target":"scryd_api::accept","category":"non-owner-user connection rejection","peer_uid":1003,"peer_pid":4711}
  ```
- Startup (INFO):
  ```
  {"ts":"2026-05-07T11:32:00.001Z","level":"info","target":"scryd_runtime::serve","kind":"startup","version":"0.1.0","commit":"abc1234","arch":"x86_64","witchcraft_features":["t5-quantized","fbgemm","hybrid-dequant"],"accounts":1,"assets_path":"/home/alice/.local/share/scryd/assets","data_dir":"/home/alice/.local/share/scryd","socket_path":"/run/user/1001/scryd/scryd.sock","idle_supported_default":true}
  ```
- API access (DEBUG):
  ```
  {"ts":"2026-05-07T11:32:05.082Z","level":"debug","target":"scryd_api::request","kind":"request","method":"GET","path":"/search","status":200,"duration_ms":17}
  ```

`SecretString` (re-stated for emphasis; lives in `scryd-config`):

- Wraps the IMAP password.
- `Debug` impl prints `SecretString([REDACTED])`.
- No `Display` impl.
- `Drop` zeroes the underlying memory.

Operator-facing journal recipes (documented in `README.install.md`, not in code):

- All errors for one account: `journalctl --user -u scryd --output cat | jq 'select(.level=="error" and .account_id=="primary")'`.
- Recent failures by category: `journalctl --user -u scryd --output cat -n 200 | jq -r '.category // empty' | sort | uniq -c | sort -rn`.
- Tail: `journalctl --user -u scryd -f`.

## §5 Sequence

1. **Daemon process start.** `scryd-runtime::serve` calls `scryd_log::init()` first, before anything else. This way any startup self-check failure (multi-instance-isolation slice) lands as a properly-formatted ERROR event in the journal rather than an uncaptured panic.
2. **Startup self-checks emit.** Every check (XDG_RUNTIME_DIR, config mode, data-dir mode) that fails emits an ERROR event with `category=configuration permission error` (or `category=configuration parse error` for malformed TOML), then the process exits non-zero. systemd journals the event before the process is reaped.
3. **Startup banner.** Once self-checks pass and resources are open, emit the `kind=startup` INFO event with the field set from Decision 9.
4. **Steady-state events.** imap-sync, search-engine, storage, mime-and-markdown, api emit through the `log_failure!` / `log_lifecycle!` macros. Each event's category/kind value comes from the closed sets in §4.
5. **API access logs.** axum middleware (api slice) wraps every handler in a span; on response, emits a `kind=request` DEBUG event for 2xx and a WARN/ERROR event for 4xx/5xx. The middleware lives in `scryd-api` but uses `scryd-log`'s macros.
6. **Shutdown.** SIGTERM handler in `scryd-runtime` triggers graceful shutdown of api, imap-sync, indexer; once the runtime is drained, emits the `kind=shutdown` INFO event with `reason`, `uptime_seconds`, `clean_close=true`. Process exits 0. systemd records the event before unit-stop.
7. **Crash path.** Uncaught panic → `panic = "abort"` (build-and-packaging) → process aborts. Journal captures whatever the panic hook printed plus systemd's "main process died" line. No "graceful" entry from us in this case; that's correct — we crashed.
8. **Crash-loop detection & backoff (next start).** Each run is recorded durably in `meta.sqlite` (`daemon_runs`, storage slice): a row with `stopped_at IS NULL` when the next process starts means the previous run aborted (panic/SIGKILL/OOM). On boot `scryd-runtime::serve` counts the trailing streak of unclean runs; once it reaches the crash-loop threshold with a recent crash, it emits a `kind=startup` WARN (`info="crash-loop detected; backing off before resuming"`, `consecutive_crashes`, `backoff_secs`, `last_crash_unix`) and sleeps with capped exponential backoff **before** reopening the indexer, so a restart storm can't burn CPU. The backoff is interruptible by SIGTERM/SIGINT (clean exit). This does not replace Decision 7's abort-on-panic — it rate-limits the loop and makes it visible instead of invisible.
9. **Crash health on `/status`.** Because the queue can read 0 rows immediately after a poison batch aborts, `/status` no longer hardcodes `ok: true`. It reports `ok = !in_crash_loop` plus a `drainer` object (`queue_depth`, `failed_permanent`, sanitized `last_index_error`) and a `daemon` object (`restart_count`, `consecutive_crashes`, `last_crash_unix`, `in_crash_loop`, `in_backoff`, `backoff_until_unix`). `scryd status` prints this JSON verbatim, so a crash loop is observable without scraping the journal.

## §6 Out of scope

- Anything inside the application logic that decides whether a failure has happened (every other slice owns its own decision-to-log).
- Log file rotation, log retention, log shipping. Journald and the operator's host policy own all of that.
- An external metrics or tracing backend (Prometheus, OpenTelemetry, Honeycomb, Sentry). v2 if needed.
- An in-app log viewer, a `scryd logs` CLI verb, or a `/logs` API endpoint. Operator uses `journalctl --user`.
- Audit-log compliance features (SOC 2, HIPAA, etc.). v2.
- Sampling of high-volume log entries. v1 emits everything; journald rate-limit is the only floor.

## §7 Open questions

- Whether `kind=backfill_progress` should fire every 1000 messages, every 5 % of the known total, or every 60 s. v1: every 1000 messages OR every 60 s, whichever comes first; tune at code time once we measure real backfill rates.
- Whether to emit a `kind=health` INFO event every N minutes summarizing per-account state for operators who scrape the journal periodically rather than `tail -f` it. v1: no — operators who want a snapshot run `journalctl --user -u scryd --since '-5min'`. **Resolved for crash health (issue #20):** rather than a periodic journal event, the durable crash/queue snapshot is exposed on demand via `GET /status` (§5 step 9), which `scryd status` prints. Periodic per-account health journaling remains out of scope.
- Whether to add a `request_id` field to api events (correlating one request's access line with its error/timing entries). v1: no — single-process daemon, span IDs are sufficient. Revisit if the daemon ever spawns subprocesses.
- Whether `non-owner-user connection rejection` should ratelimit (a hostile process could flood the journal by repeatedly connecting). v1: no application-level ratelimit; journald handles it. Confirm we're comfortable with the worst-case "journal of length N" bound under attack.
- Whether to ship a `scryd-log-jq-helpers.json` companion file with the operator-facing journal recipes from §4 baked into `jq` aliases. v1: documented as text in `README.install.md`; ship the jq helpers if early operators ask.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
