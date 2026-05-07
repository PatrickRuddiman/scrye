Parent slice: [api](../slices/api.md)
Depends on: 18, 06, 15

# Task 19 — scryd-api-writes

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the three write/admin endpoints — `POST /sync`, `POST /internal/reindex`, `POST /internal/reconcile` — including the daemon-wide single-flight reindex mutex.

## Tasks
- [ ] In `crates/scryd-api/src/handlers/sync.rs`, implement `pub async fn handle_sync(State(state)) -> Response`: call `state.scheduler.request_pass()` → return `202 Accepted` with body `{"started": true, "scope": {"accounts": [...]}}` listing the account ids that were signaled.
- [ ] In `crates/scryd-api/src/handlers/reindex.rs`, implement `pub async fn handle_reindex(State(state)) -> Response`. Acquire `state.reindex_lock`; if already held, return `409 conflict` with `{"error":{"code":"conflict","message":"a reindex is already in progress"}}`. Otherwise: call `state.witchcraft.truncate()`; call `state.storage.reenqueue_all_messages()`; release the lock; return `202 Accepted` with `{"started": true}`. The actual rebuild proceeds in the drainer task; the handler doesn't wait.
- [ ] Define `pub struct ReindexHandle { started_at: Instant, accounts_count: u64 }` so future logic can decide if a stale reindex should be force-released. v1 doesn't force-release; the field exists for v2 hooks.
- [ ] In `crates/scryd-api/src/handlers/reconcile.rs`, implement `pub async fn handle_reconcile(State(state)) -> Response`: reload config from `Config::load_from_xdg()` (storage holds an `Arc<RwLock<Config>>` that the runtime updates) → call `storage.reconcile_from_config(&new_config)` → call `state.scheduler.reconcile()` → return `202 Accepted` with `{"started": true, "accounts_added": [...], "accounts_inactivated": [...]}`.
- [ ] Update `crates/scryd-runtime/src/serve.rs` (Task 16) to wrap the `Config` in `Arc<RwLock<Config>>` so the reconcile handler can hot-swap it without restart. Document that the password field is re-read from disk each reconcile and never cached in the daemon beyond the imap-sync supervisor's per-login lifetime.
- [ ] In `crates/scryd-api/src/router.rs`, mount the three POST routes: `/sync`, `/internal/reindex`, `/internal/reconcile`.
- [ ] Write integration tests in `crates/scryd-api/tests/handlers_write.rs`: seed an api with a stub scheduler that records calls; assert `POST /sync` returns 202 and the scheduler observed exactly one `request_pass` call; assert `POST /internal/reindex` returns 202 and the witchcraft stub saw `truncate` + the storage saw `reenqueue_all_messages`; second concurrent `POST /internal/reindex` while the first is still holding the lock returns 409; `POST /internal/reconcile` returns 202 with the diff after a config-on-disk change.
- [ ] Write a unit test asserting the single-flight semantics: two concurrent `tokio::spawn` calls to `handle_reindex` against the same `AppState` — exactly one returns 202, the other returns 409.

## Acceptance criteria
- [ ] `cargo test -p scryd-api --test handlers_write` passes.
- [ ] `cargo check -p scryd-api` exits 0.
- [ ] `git grep -nE 'reindex_lock' crates/scryd-api/src/state.rs` matches.
- [ ] `git grep -nE '"conflict"|StatusCode::CONFLICT|409' crates/scryd-api/src/handlers/reindex.rs` matches.
- [ ] `git grep -nE 'reenqueue_all_messages' crates/scryd-api/src/handlers/reindex.rs` matches.
- [ ] `git grep -nE 'scheduler\.reconcile' crates/scryd-api/src/handlers/reconcile.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
