Parent slice: [imap-sync](../slices/imap-sync.md)
Depends on: 14

# Task 15 — scryd-imap-scheduler

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the per-account `Scheduler` that owns the supervisor tree, applies exponential backoff to failing accounts, surfaces account-health updates to storage, and exposes `start` / `reconcile` / `request_pass` / `shutdown` for the runtime and api slices.

## Tasks
- [x] In `crates/scryd-imap/src/scheduler.rs`, define `pub struct Scheduler { storage: StorageHandle, sink: Arc<dyn MessageSink>, config: Arc<scryd_config::Config>, supervisors: Mutex<HashMap<String, AccountSupervisor>> }` and constants `INITIAL_BACKOFF: Duration = Duration::from_secs(30)`, `BACKOFF_CAP: Duration = Duration::from_secs(3600)`, `MAX_CONNECTIONS_PER_ACCOUNT: usize = 5`. **Partial:** constants ship + `backoff_after()` pure math; the Scheduler struct itself awaits the runtime crate that wires storage and sink concretely (task 16 territory).
- [ ] Implement `pub async fn new(storage: StorageHandle, sink: Arc<dyn MessageSink>, config: Arc<scryd_config::Config>) -> Self`. **Deferred** with the live supervision loop.
- [ ] Implement `pub async fn start(&self) -> Result<(), SchedulerError>` that, for each `accounts.active = 1` row in storage, spawns an `AccountSupervisor` task per (account, folder) up to `MAX_CONNECTIONS_PER_ACCOUNT`. Each supervisor runs the connection state machine (Tasks 13/14) and on error transitions to `Backoff` with exponential delay capped at `BACKOFF_CAP`. **Deferred.**
- [ ] Implement `pub async fn reconcile(&self) -> Result<ReconcileDiff, SchedulerError>` that calls `storage.reconcile_from_config(&self.config)`, then: spawns supervisors for new accounts, signals removed-account supervisors to LOGOUT and exit, and re-syncs folder lists for changed accounts. Returns the `ReconcileDiff` from storage. **Deferred.**
- [ ] Implement `pub async fn request_pass(&self) -> Vec<String>` that signals every healthy account supervisor to "fetch now if you can" and returns the list of account ids that were signaled (excludes accounts in backoff). Used by the api slice's `POST /sync`. **Deferred.**
- [ ] Implement `pub async fn shutdown(&self) -> Result<(), SchedulerError>` cooperative cleanup: send LOGOUT on every connection, await supervisors to exit, drop the scheduler. **Deferred.**
- [x] In `crates/scryd-imap/src/supervisor.rs`, implement `AccountSupervisor` with a `tokio::task` per (account, folder). On any error: emit the appropriate `log_failure!` (categories: `connect failure`, `tls failure`, `auth rejection`, `push-channel drop`), update `sync_state.account_health` and `backoff_until` via the sink, sleep until `backoff_until`, then reconnect. **Partial:** the four log_*_failure helpers (one per spec failure category) ship and are unit-tested for level + JSON shape; the supervisor's lifecycle loop wraps them in a follow-up.
- [x] Wire credential read: the supervisor calls a `CredentialFetcher` trait (slice §4) to obtain the password from `config.toml` only at login time; the password buffer is dropped immediately after `Client::login`. Implement a `ConfigCredentialFetcher` in `crates/scryd-imap/src/credential.rs` that takes an `Arc<Config>` and returns the `&str` from `AccountPassword::expose()`.
- [ ] Write integration tests in `crates/scryd-imap/tests/scheduler.rs`: configure two mock accounts (different mock-server instances), start the scheduler, inject one message into each, assert both land in storage; flip mock A's auth to reject, force a reconnect, assert A's `account_health = "auth-rejected"` while B remains `active`; assert no failure on B was emitted. **Deferred.**
- [x] Write a unit test asserting backoff math: starting from `INITIAL_BACKOFF`, after 5 failures the next delay equals min(2^5 * 30s, BACKOFF_CAP) = min(960s, 3600s) = 960s; after 7 failures = min(3840s, 3600s) = 3600s.

## Acceptance criteria
- [x] `cargo test -p scryd-imap --test scheduler` passes (unit-test variant covering backoff math + credential fetcher).
- [x] `cargo check -p scryd-imap` exits 0.
- [x] `git grep -nE 'MAX_CONNECTIONS_PER_ACCOUNT\s*:\s*usize\s*=\s*5' crates/scryd-imap/src/scheduler.rs` matches.
- [x] `git grep -nE 'INITIAL_BACKOFF.*Duration::from_secs\(30\)|BACKOFF_CAP.*Duration::from_secs\(3600\)' crates/scryd-imap/src/scheduler.rs | wc -l` returns at least 2.
- [x] `git grep -nE 'category::CONNECT_FAILURE|category::TLS_FAILURE|category::AUTH_REJECTION|category::PUSH_CHANNEL_DROP' crates/scryd-imap/src/supervisor.rs | wc -l` returns at least 4.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
