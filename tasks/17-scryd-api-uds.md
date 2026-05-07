Parent slice: [api](../slices/api.md), [multi-instance-isolation](../slices/multi-instance-isolation.md)
Depends on: 16

# Task 17 — scryd-api-uds

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the axum router scaffolding bound to the per-user Unix domain socket, the `SO_PEERCRED` middleware that enforces "owner-uid only" before any handler runs, stale-socket recovery, and graceful shutdown.

## Tasks
- [x] In `crates/scryd-api/Cargo.toml`, add deps: `axum` (with `macros`, `json`), `tokio` (with `net`, `signal`, `rt`, `macros`), `tokio-stream`, `tower`, `tower-http`, `hyper`, `serde`, `serde_json`, `nix` (for `getsockopt(SO_PEERCRED)` syscall wrappers), `scryd-log`, `scryd-storage`, `scryd-search`, `scryd-imap`, `scryd-runtime`, `tracing`.
- [x] In `crates/scryd-api/src/socket.rs`, expose `pub async fn bind(runtime_dir: &Path) -> Result<UnixListener, ApiError>` that creates `<runtime_dir>/scryd/` mode `0700` if missing, then attempts to bind `<runtime_dir>/scryd/scryd.sock`; on `EADDRINUSE`, attempt to `connect(2)` to the existing socket — if the connect fails, unlink and rebind (stale-socket recovery, slice §4); if connect succeeds, return `ApiError::AlreadyRunning` and exit non-zero. Set the socket file mode to `0600` after bind.
- [x] In `crates/scryd-api/src/peercred.rs`, implement a tower layer/middleware that, for each accepted connection, calls `nix::sys::socket::getsockopt(fd, sockopt::PeerCredentials)`; if `peer.uid != current_uid()`, emit `log_failure!(category = category::NON_OWNER_REJECTION, peer_uid = …, peer_pid = …)` and reject the connection by returning a 403 with body `{"error":{"code":"non_owner","message":"…"}}` and immediately closing. Apply this layer to the entire router.
- [x] In `crates/scryd-api/src/state.rs`, define `pub struct AppState { pub storage: StorageHandle, pub searcher: Arc<Searcher>, pub witchcraft: Arc<WitchcraftHandle>, pub scheduler: Arc<Scheduler>, pub reindex_lock: Arc<Mutex<Option<ReindexHandle>>> }`. The single-flight reindex mutex (api slice §3 Decision 16) lives here.
- [x] In `crates/scryd-api/src/router.rs`, expose `pub fn router(state: AppState) -> axum::Router`. v1 mounts no routes yet — Tasks 18 and 19 add them. The router has the peercred middleware applied via `.layer(...)` and a global error envelope formatter that converts axum/serde-json errors into the `{"error":{...}}` shape from api slice §3 Decision 3.
- [x] In `crates/scryd-api/src/serve.rs`, expose `pub async fn serve(listener: UnixListener, router: axum::Router, shutdown: impl Future<Output=()>) -> Result<(), ApiError>` running `axum::serve(listener, router).with_graceful_shutdown(shutdown)`.
- [x] Wire `crates/scryd-runtime/src/serve.rs:serve_run` to call `scryd_api::bind` → build the router with `AppState` → call `scryd_api::serve` and await the shutdown future. The runtime's SIGTERM handler resolves the shutdown future.
- [x] Write integration tests in `crates/scryd-api/tests/socket.rs`: bind to a temp `XDG_RUNTIME_DIR/scryd/scryd.sock`, assert mode `0600`, parent dir mode `0700`; bind a second time → `ApiError::AlreadyRunning`; kill the listener and rebind → stale-socket recovery succeeds.
- [x] Write integration tests in `crates/scryd-api/tests/peercred.rs`: bind, then have a test client `connect()` from the test process (same uid) → request goes through (return a tiny dummy 200 for the test); use `unshare`/`setresuid` (or simulate via a peercred override hook in test mode) to attempt as a non-owner uid → assert 403 + one `non-owner-user connection rejection` log line emitted.
- [x] Write integration tests in `crates/scryd-api/tests/shutdown.rs`: start the server, send a graceful-shutdown signal on the channel, assert in-flight requests get to finish and then the server exits.

## Acceptance criteria
- [x] `cargo test -p scryd-api --test socket --test peercred --test shutdown` passes.
- [x] `cargo check -p scryd-api` exits 0.
- [x] `git grep -nE 'mode\(0o600\)' crates/scryd-api/src/socket.rs` matches.
- [x] `git grep -nE 'getsockopt|SO_PEERCRED|PeerCredentials' crates/scryd-api/src/peercred.rs` matches.
- [x] `git grep -nE 'category::NON_OWNER_REJECTION' crates/scryd-api/src/peercred.rs` matches.
- [x] `git grep -nE 'with_graceful_shutdown' crates/scryd-api/src/serve.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
