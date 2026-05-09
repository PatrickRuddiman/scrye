Parent plan: GreenMail integration testing for scryd
Parent v0.1.0 task: [16-scryd-runtime-serve.md](../16-scryd-runtime-serve.md)
Depends on: 07

# Task 08 — runtime-serve

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land `scryd_runtime::serve()` (and the split `serve_init` / `serve_run` per the v0.1.0 task 16 spec) and rewire `scryd/src/main.rs:115-121` `run_serve` to call it. After this task, `scryd serve` is no longer a stub — it boots logging + preflight + storage + scheduler + indexer drainer + api router and stays up until SIGTERM.

## Tasks
- [ ] In `crates/scryd-runtime/src/serve.rs`, implement `pub async fn serve_init() -> Result<ServeContext, RuntimeError>` per the task 16 spec: scryd_log::init() → preflight::run() → Config::load_from_xdg() → open StorageHandle → reconcile accounts → construct WitchcraftHandle → construct Drainer + spawn → construct StorageMessageSink + Scheduler::new → scheduler.start() → emit `kind::STARTUP` log_lifecycle.
- [ ] Implement `pub async fn serve_run(ctx: ServeContext) -> Result<(), RuntimeError>`: install SIGTERM/SIGINT handlers via `tokio::signal::unix::signal` → await shutdown signal → cooperative shutdown of api / scheduler / drainer / storage → emit `kind::SHUTDOWN`.
- [ ] Implement `pub async fn serve() -> Result<(), RuntimeError>` as `serve_run(serve_init().await?).await`.
- [ ] Define `pub struct ServeContext { storage, scheduler, drainer, witchcraft, api_router }` exposing the components task 17 will need.
- [ ] Install a panic hook before any tokio task spawns: log `category-less level=error` and abort.
- [ ] In `scryd/src/main.rs:115-121`, replace the `run_serve` stub with:
  ```
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
  match rt.block_on(scryd_runtime::serve()) {
      Ok(()) => std::process::exit(0),
      Err(e) => bail(ExitCode::Error, "error", &e.to_string()),
  }
  ```
- [ ] In `crates/scryd-runtime/tests/serve.rs`, write the deferred test: temp XDG layout + one-account config (`tls = false`, host = `127.0.0.1`, port = 3143) + start GreenMail-fixture-aware test (skip-if-unreachable). Call `serve_init`, assert scheduler is started + drainer is alive + startup banner emitted (capture via custom log writer). Send a fake SIGTERM via `tokio::signal::unix::signal(SignalKind::terminate())` and assert clean shutdown.
- [ ] Verify the v0.2.0 isolation property is preserved: when running under euid 0 (sudo or systemd User=scryd), `serve()` calls `peercred::init()` from scryd-api before opening the listener (already wired by v0.2.0 task 01).

## Acceptance criteria
- [ ] `cargo test -p scryd-runtime --test serve` passes locally with GreenMail.
- [ ] `cargo build --release -p scryd` succeeds.
- [ ] `target/release/scryd serve` (with a valid config + GreenMail running) stays up; `kill -TERM <pid>` exits cleanly.
- [ ] `git grep -nE 'pub async fn serve_init|pub async fn serve_run|pub async fn serve' crates/scryd-runtime/src/serve.rs | wc -l` returns at least 3.
- [ ] `! grep -F 'live integration deferred' scryd/src/main.rs` (the stub is gone).
- [ ] `git grep -nE 'kind::STARTUP|kind::SHUTDOWN' crates/scryd-runtime/src/serve.rs | wc -l` returns at least 2.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
