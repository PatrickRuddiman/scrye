Parent slice: [observability](../slices/observability.md)
Depends on: 00

# Task 02 — scryd-log

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Provide the daemon-wide tracing initialization, JSON-Lines vs pretty subscriber selection, and the `log_failure!` / `log_lifecycle!` macros that pin every spec-closed-set category and kind value.

## Tasks
- [ ] In `crates/scryd-log/Cargo.toml`, add deps: `tracing`, `tracing-subscriber` (with `env-filter`, `json`, `fmt` features), `serde`, `serde_json`, `is-terminal`, `once_cell`.
- [ ] In `crates/scryd-log/src/lib.rs`, expose `pub fn init() -> Result<(), InitError>`. It reads `RUST_LOG` (default `scryd=info,scryd_api=info,scryd_imap=info,scryd_search=info,scryd_storage=info,scryd_mime=info,scryd_runtime=info,warn`), detects whether stderr is a TTY via `is_terminal::IsTerminal::is_terminal(&std::io::stderr())`, installs a JSON `tracing_subscriber::fmt::layer()` for non-TTY or a pretty layer for TTY. Idempotent — second call returns `InitError::AlreadyInitialized`.
- [ ] In `crates/scryd-log/src/categories.rs`, define `pub mod category` with one `pub const` per spec category exactly as written: `CONNECT_FAILURE = "connect failure"`, `TLS_FAILURE = "tls failure"`, `AUTH_REJECTION = "auth rejection"`, `PUSH_CHANNEL_DROP = "push-channel drop"`, `UID_VALIDITY_RESET = "uid-validity reset triggering re-sync"`, `SINGLE_MESSAGE_PARSE_FAILURE = "single-message parse failure"`, `SINGLE_MESSAGE_FT_INDEXER_FAILURE = "single-message full-text indexer failure"`, `SINGLE_MESSAGE_SEMANTIC_INDEXER_FAILURE = "single-message semantic indexer failure"`, `DISK_FULL = "disk-full"`, `CONFIG_PARSE_ERROR = "configuration parse error"`, `CONFIG_PERMISSION_ERROR = "configuration permission error"`, `NON_OWNER_REJECTION = "non-owner-user connection rejection"`. Define `pub mod kind` with `STARTUP`, `SHUTDOWN`, `SYNC_PASS_START`, `SYNC_PASS_COMPLETE`, `BACKFILL_PROGRESS`, `IDLE_ENTER`, `IDLE_DROP`, `IDLE_RECYCLE`, `UIDVALIDITY_RESET`, `REINDEX_START`, `REINDEX_COMPLETE`, `ACCOUNT_RECONCILED`, `REQUEST` const strings.
- [ ] In `crates/scryd-log/src/macros.rs`, define `log_failure!` as a `#[macro_export]` `macro_rules!` that expands to a `tracing::event!` with `Level::ERROR` (or `WARN` when severity is overridden) and includes the literal `category` field plus any caller-supplied fields. Define `log_lifecycle!` similarly for INFO + `kind` field. Macros take `severity = error|warn|info|debug` as a leading optional token.
- [ ] In `crates/scryd-log/src/macros.rs`, also expose `log_request!` as a `#[macro_export]` macro for the api access log (Decision 11), expanding to `tracing::event!(Level::DEBUG, kind = kind::REQUEST, …)`.
- [ ] Write unit tests in `crates/scryd-log/tests/integration.rs`: install the subscriber with a custom JSON writer captured into a `Vec<u8>`; emit `log_failure!(category = category::AUTH_REJECTION, account_id = "primary")`; assert the captured bytes deserialize to JSON containing `"level":"error"`, `"category":"auth rejection"`, `"account_id":"primary"`. Do the same for `log_lifecycle!(kind = kind::STARTUP, version = "0.1.0")` asserting `"level":"info"` and `"kind":"startup"`.
- [ ] Write unit test asserting `init()` called twice returns `InitError::AlreadyInitialized` on the second call.

## Acceptance criteria
- [ ] `cargo test -p scryd-log` passes.
- [ ] `cargo check -p scryd-log` exits 0.
- [ ] `git grep -nE '"connect failure"|"tls failure"|"auth rejection"|"push-channel drop"|"uid-validity reset triggering re-sync"|"single-message parse failure"|"single-message full-text indexer failure"|"single-message semantic indexer failure"|"disk-full"|"configuration parse error"|"configuration permission error"|"non-owner-user connection rejection"' crates/scryd-log/src/categories.rs | wc -l` outputs `12`.
- [ ] `cargo run -p scryd-log --example emit-failure 2>&1 | grep -F '"category":"auth rejection"'` succeeds — add `crates/scryd-log/examples/emit-failure.rs` that calls `init()` then emits one failure log line.
- [ ] `test -f crates/scryd-log/src/categories.rs && test -f crates/scryd-log/src/macros.rs`.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
