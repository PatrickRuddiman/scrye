---
sources:
  - crates/scryd-runtime/src/serve.rs
  - crates/scryd-runtime/src/preflight.rs
  - crates/scryd-storage/src/daemon.rs
  - crates/scryd-storage/src/migrations/v2_daemon_runs.rs
  - ops/scryd.service.in
---

# Daemon lifecycle

This page describes how the daemon starts, records each run, throttles crash
loops, and shuts down. The runtime drives all of it from
`scryd_runtime::serve()`.

## Startup order

`serve()` initializes in a fixed order and fails fast if any step does not hold:

1. Install a panic hook and initialize logging (see [Observability](observability.md)).
2. Run preflight checks (below).
3. Resolve `USER_EMAIL` from the environment. The daemon exits if it is empty —
   there is no mailbox to scope to.
4. Resolve the config path, load and validate the TOML, and retain only the
   account(s) whose login matches `USER_EMAIL`. A warning is logged if none
   match. See [Configuration](../configuration.md).
5. Resolve the MCP bind address. It must be a loopback address; otherwise the
   daemon exits.
6. Create the data directory and open `meta.sqlite` with a small read pool.
7. Record this run in `daemon_runs` and compute crash-loop backoff (below).
8. Ensure the witchcraft asset bundle is present, fetching it on first start if
   missing. See [Weights and assets](../operations/weights-and-assets.md).
9. Seed `accounts` rows from config, open the witchcraft index
   (`witchcraft.sqlite`), and spawn the drainer, the scheduler, and the MCP
   server.

## Preflight

Preflight (`scryd-runtime/src/preflight.rs`) enforces two on-disk invariants
before any heavy work:

- The config file must exist and carry no world-permission bits. A world-
  readable or world-writable config is rejected.
- The data directory is created if missing and must be a directory with mode
  `0700`.

A failure logs a configuration-permission error and aborts startup with a
permission-invariant error. See [Configuration](../configuration.md) for the
permission rules and [Security](../security.md) for the rationale.

## Run accounting

The daemon records one row per process in the `daemon_runs` table so crashes are
durable across restarts (`scryd-storage/src/daemon.rs`):

- On start, `begin_run` inserts a row with `started_at = now`, `stopped_at =
  NULL`, and `clean = 0`, then returns the crash history that preceded it.
- A run whose `stopped_at` is still `NULL` when the next process starts is a run
  that died without a clean shutdown (panic, abort, `SIGKILL`, or OOM).
- `begin_run` counts the trailing streak of unclean runs (newest first, stopping
  at the first clean run) and remembers when the most recent crash started.
- `run_id` is `AUTOINCREMENT`; pruning keeps the most recent 200 rows without
  recycling ids, so the maximum `run_id` stays a monotonic lifetime restart
  counter.
- On graceful shutdown, `finish_run` sets `stopped_at = now` and `clean = 1` so
  the next process does not count this run as a crash.

These numbers surface directly in the MCP [`status`](../mcp/tools/status.md)
tool's `daemon` object (`restart_count`, `consecutive_crashes`,
`last_crash_unix`, `in_crash_loop`, `backoff_until_unix`).

## Crash-loop backoff

When recent runs keep dying uncleanly, the daemon throttles itself before
reopening the heavy index. The backoff is computed in `serve.rs` from the run
stats:

- It applies only once there are at least **3** consecutive unclean runs and the
  most recent crash is within a **600-second** window of now. Below the
  threshold, or after a quiet period, there is no backoff.
- The delay starts at **5 seconds** and doubles per additional crash beyond the
  threshold, capped at **300 seconds**: 5s, 10s, 20s, 40s, … up to the cap.
- The sleep happens before the indexer opens and the drainer spawns, so it
  throttles every crash site, and it is interruptible by `SIGTERM`/`SIGINT`.
  Interrupting during a backoff marks the run clean and exits cleanly, so an
  aborted-early start does not inflate the next process's crash streak.

While in backoff, the [`status`](../mcp/tools/status.md) tool reports
`ok: false` so the crash loop stays visible to clients.

The shipped systemd unit (`ops/scryd.service.in`) uses `Restart=on-failure` with
`RestartSec=5` and deliberately does not set `StartLimitIntervalSec` /
`StartLimitBurst`: parking the unit would hide the crash-loop signal that
`status` is meant to surface.

## Shutdown

`serve()` waits for `SIGTERM` or `SIGINT`, then shuts down in order: cancel the
MCP server, stop the scheduler, cancel the drainer, and call `finish_run` to mark
the run clean. A shutdown event is logged with `clean_close = true`.

## See also

- [Architecture](architecture.md)
- [Configuration](../configuration.md)
- [Observability](observability.md)
- [status tool](../mcp/tools/status.md)
- [Weights and assets](../operations/weights-and-assets.md)
