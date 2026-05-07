Parent slice: [cli](../slices/cli.md)
Depends on: 20, 19

# Task 23 — scryd-cli-reindex

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the fire-and-forget `scryd reindex` verb: dispatches `POST /internal/reindex`, maps responses to the cli slice's exit codes, no progress streaming.

## Tasks
- [x] In `scryd/src/cmd_reindex.rs`, define a no-arg clap subcommand. Implement `pub async fn run() -> Result<(), CliError>`.
- [x] Build a `UdsClient`. POST to `/internal/reindex` with no body. Map responses:
  - `202 Accepted` → print `reindex started; search continues to serve during rebuild` and `check progress with: journalctl --user -u scryd` to stdout, exit 0.
  - `409 Conflict` → print `a reindex is already running` to stdout, exit 0 (intentional — operator's intent already in flight).
  - `daemon_not_running` (UDS connect error) → print `scryd: daemon-not-running: scryd is not running for user $USER. Start it with: systemctl --user start scryd` to stderr, exit 2.
  - Other 4xx → print `scryd: daemon-rejected: <api error message>` to stderr, exit 5.
  - Other 5xx → print `scryd: daemon-rejected: internal_error` to stderr, exit 5.
- [x] Wire into the dispatcher in `scryd/src/main.rs`.
- [x] Write integration tests in `scryd/tests/cmd_reindex.rs` using a stub api on a temp UDS:
  - stub returns 202 → `assert_cmd` invocation: stdout contains "reindex started", exit code 0.
  - stub returns 409 → stdout contains "already running", exit code 0.
  - no stub running → stderr contains "daemon-not-running", exit code 2.
  - stub returns 500 → stderr contains "daemon-rejected", exit code 5.

## Acceptance criteria
- [x] `cargo test -p scryd --test cmd_reindex` passes.
- [x] `cargo build --release -p scryd` exits 0.
- [x] `target/release/scryd reindex --help 2>&1 | grep -E 'Usage:.*reindex'` matches (no flags expected; just the verb).
- [x] `git grep -nE 'StatusCode::ACCEPTED|202' scryd/src/cmd_reindex.rs` matches.
- [x] `git grep -nE 'StatusCode::CONFLICT|409' scryd/src/cmd_reindex.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
