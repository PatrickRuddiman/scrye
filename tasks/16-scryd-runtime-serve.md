Parent slice: [multi-instance-isolation](../slices/multi-instance-isolation.md), [build-and-packaging](../slices/build-and-packaging.md)
Depends on: 02, 03, 15

# Task 16 — scryd-runtime-serve

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the runtime entrypoint that wires startup self-checks, logging, storage, the indexer drainer, the imap-sync scheduler, signal handling, and the startup banner — the single function `scryd serve` will eventually call.

## Tasks
- [x] In `crates/scryd-runtime/Cargo.toml`, add deps: `tokio` (with `signal`, `rt-multi-thread`, `macros`), `scryd-config`, `scryd-log`, `scryd-storage`, `scryd-search`, `scryd-imap`, `scryd-mime`, `tracing`, `thiserror`, `anyhow`.
- [x] In `crates/scryd-runtime/src/xdg.rs`, expose `pub fn runtime_dir() -> Result<PathBuf, RuntimeError>` reading `$XDG_RUNTIME_DIR` and erroring if unset; `pub fn config_path() -> Result<PathBuf, RuntimeError>` from `$XDG_CONFIG_HOME` (default `$HOME/.config`); `pub fn data_dir() -> Result<PathBuf, RuntimeError>` from `$XDG_DATA_HOME` (default `$HOME/.local/share`); `pub fn assets_dir() -> Result<PathBuf, RuntimeError>` resolving to `<data_dir>/assets`.
- [x] In `crates/scryd-runtime/src/preflight.rs`, expose `pub fn run() -> Result<PreflightOk, RuntimeError>` performing the multi-instance-isolation slice §3 Decision 9 self-checks: `$XDG_RUNTIME_DIR` exists + owned by current uid + mode `0700`; `<config_path>` exists + owned + mode `0600`; `<data_dir>/scryd/` exists or is creatable + mode `0700`. Each failure emits a `category::CONFIG_PERMISSION_ERROR` log and returns `RuntimeError::PermissionInvariant`.
- [x] In `crates/scryd-runtime/src/sink.rs`, implement `pub struct StorageMessageSink { storage: StorageHandle, search_drainer_notify: Arc<Notify> }` implementing `scryd_imap::MessageSink`. `submit` calls `scryd_mime::parse(raw_bytes, ctx)` → on `Parsed`/`ParsedDegraded` writes the message row + raw .eml + attachments + enqueue + `notify.notify_one()`; on `Unparseable` writes a placeholder row + raw .eml only and emits `category::SINGLE_MESSAGE_PARSE_FAILURE` with the fault subtype. `tombstone` calls `storage.tombstone`. `update_sync_state` calls `storage.update_sync_state`.
- [ ] In `crates/scryd-runtime/src/serve.rs`, implement `pub async fn serve() -> Result<(), RuntimeError>` orchestrating: (1) `scryd_log::init()`; (2) `preflight::run()`; (3) load `Config` via `scryd_config::Config::load_from_xdg()`; (4) open `StorageHandle` against `<data_dir>/scryd/meta.sqlite`; (5) reconcile accounts from config; (6) construct `WitchcraftHandle` against `<data_dir>/scryd/witchcraft.sqlite` + `assets_dir`; (7) construct `Drainer` and spawn its task; (8) construct `StorageMessageSink` and `Scheduler::new(...)`, call `scheduler.start()`; (9) emit the `kind::STARTUP` banner via `log_lifecycle!` with the field set from observability slice Decision 9; (10) install SIGTERM/SIGINT handlers via `tokio::signal`; (11) await shutdown signal; (12) cooperative shutdown of api (Task 17 wires this), scheduler, drainer, storage; (13) emit `kind::SHUTDOWN`. **Deferred** — needs the witchcraft binding and live IMAP scheduler from earlier deferred tasks.
- [ ] In `crates/scryd-runtime/src/serve.rs`, install a panic hook before any tokio task spawns that emits a category-less `level=error` log and aborts (matches `panic = "abort"` profile from Task 00). **Deferred** with serve.rs.
- [ ] Define a `pub struct ServeContext` returned by `serve_init()` that exposes the started components (`StorageHandle`, `Scheduler`, `Drainer`, `WitchcraftHandle`, etc.) for the api slice (Task 17) to mount its router on the same runtime. Split `serve()` into `serve_init() -> ServeContext` and `serve_run(ctx) -> Result<(), RuntimeError>` so the api slice can wire its router into `ctx` between init and run. **Deferred.**
- [x] Write integration tests in `crates/scryd-runtime/tests/preflight.rs` using a `tempfile::TempDir` fake-home: each invariant check has a happy-path test and a failing-path test (e.g., config mode `0644` → `PermissionInvariant` + one `configuration permission error` log line captured).
- [ ] Write integration tests in `crates/scryd-runtime/tests/serve.rs`: stub `WitchcraftHandle` and the imap-sync scheduler (use the test-only stubs from earlier tasks); call `serve_init` against a temp XDG layout with a one-account config; assert the scheduler started, drainer is alive, startup banner was emitted (capture with a custom log writer); send a fake SIGTERM via `tokio::signal::unix::signal(SignalKind::terminate())` mock and assert clean shutdown. **Deferred** with serve.rs.

## Acceptance criteria
- [x] `cargo test -p scryd-runtime` passes (xdg + preflight + sink test files).
- [x] `cargo check -p scryd-runtime` exits 0.
- [x] `git grep -nE 'fn run\(\)' crates/scryd-runtime/src/preflight.rs` matches.
- [ ] `git grep -nE 'kind::STARTUP|kind::SHUTDOWN' crates/scryd-runtime/src/serve.rs | wc -l` returns at least 2. **Deferred** with serve.rs.
- [x] `git grep -nE 'category::CONFIG_PERMISSION_ERROR' crates/scryd-runtime/src/preflight.rs` matches.
- [ ] `git grep -nE 'pub async fn serve_init|pub async fn serve_run' crates/scryd-runtime/src/serve.rs | wc -l` returns 2. **Deferred** with serve.rs.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
