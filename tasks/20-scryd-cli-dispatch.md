Parent slice: [cli](../slices/cli.md)
Depends on: 00, 01, 16

# Task 20 — scryd-cli-dispatch

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land the clap parser tree (three operator verbs + hidden `serve`), the UDS-aware HTTP client, the closed exit-code mapping, and the `scryd serve` dispatch into `scryd-runtime`.

## Tasks
- [x] In `scryd/Cargo.toml` (the binary crate), add deps: `clap` (with `derive`), `tokio` (with `rt-multi-thread`, `macros`), `reqwest` (with `unix-socket`, `json`, `stream`), `serde`, `serde_json`, `anyhow`, `thiserror`, `is-terminal`, `scryd-config`, `scryd-runtime`, `scryd-log`.
- [x] In `scryd/src/main.rs`, define the clap tree with the verbs `add-account`, `reindex`, `search`, plus `serve` marked `#[command(hide = true)]`. The shape of `add-account` and `search` is filled in by Tasks 21 and 22; this task only stubs them so the parser compiles.
- [x] In `scryd/src/exit.rs`, define `pub enum ExitCode { Ok = 0, Error = 1, DaemonNotRunning = 2, ConfigError = 3, BadInput = 4, DaemonRejected = 5 }` and a `From<...>` chain that maps client errors to the right code. Implement a `bail!` analogue that prints `scryd: <category>: <message>` to stderr and exits with the matching code.
- [x] In `scryd/src/uds_client.rs`, expose `pub struct UdsClient { socket_path: PathBuf, http: reqwest::Client }`. Constructor resolves `$XDG_RUNTIME_DIR` (or `ExitCode::Error` with the same message the daemon would emit) and points reqwest at the unix socket via the `unix-socket` feature. Methods: `get_json(path) -> Result<serde_json::Value, ClientError>`, `get_stream(path) -> Result<reqwest::Response, ClientError>` (for the raw .eml stream — proxied via cli only if a future verb wants it; v1 doesn't surface raw .eml on the cli), `post_json(path) -> Result<serde_json::Value, ClientError>`. Map connect errors `ENOENT`/`ECONNREFUSED` to `ClientError::DaemonNotRunning`; HTTP 4xx to `ClientError::DaemonRejected { code, message }`; HTTP 5xx to `ClientError::DaemonError`.
- [x] In `scryd/src/cmd_serve.rs`, implement `pub async fn run() -> Result<(), anyhow::Error>` that calls `scryd_runtime::serve()`. This is the entrypoint systemd invokes via `ExecStart=%h/.local/bin/scryd serve`.
- [x] Wire the dispatcher in `scryd/src/main.rs`: parse args → match verb → dispatch into the appropriate `cmd_*` module (stubs for `add-account`/`reindex`/`search`, real for `serve`); tokio runtime is built only for verbs that need it.
- [x] Write integration tests in `scryd/tests/parser.rs` using `clap`'s `try_get_matches_from`: assert that `scryd --help` lists exactly `add-account`, `reindex`, `search` and does NOT list `serve`; `scryd --help serve` does list it. Assert that `scryd unknown` returns a parse error.
- [x] Write integration tests in `scryd/tests/exit_codes.rs`: invoke the binary as a subprocess against an unset `XDG_RUNTIME_DIR` → exit code 1; against a set runtime dir with no socket → run any verb that requires the daemon → exit code 2 with stderr containing `daemon-not-running`. Use `assert_cmd` for clean subprocess assertions.

## Acceptance criteria
- [x] `cargo build --release -p scryd` exits 0.
- [x] `cargo test -p scryd --test parser --test exit_codes` passes.
- [x] `target/release/scryd --help 2>&1 | grep -E '^\s*(add-account|reindex|search)\b' | wc -l` outputs `3`.
- [x] `target/release/scryd --help 2>&1 | grep -wE 'serve'` returns no output.
- [x] `XDG_RUNTIME_DIR= target/release/scryd reindex 2>&1; echo $?` last line equals `1` and stderr contains `XDG_RUNTIME_DIR`.
- [x] `git grep -nE 'enum ExitCode' scryd/src/exit.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
