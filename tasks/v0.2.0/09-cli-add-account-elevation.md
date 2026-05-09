Parent slice: [scryd v0.2.0 — cli](../../slices/0.2.0/cli.md)
Depends on: 08

# Task 09 — cli-add-account-elevation

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Add an elevation precondition to `scryd add-account`, chown the rewritten config to `scryd:scryd` after the atomic write so the daemon (running under that UID) keeps read access, and print a context-sensitive restart-vs-start hint depending on whether the daemon is currently reachable.

## Tasks
- [ ] In `scryd/src/main.rs` `run_add_account` (line 221+), before any prompt or argument parsing that reads sensitive input: if `nix::unistd::geteuid().as_raw() != 0`, call `bail(ExitCode::BadInput, "bad-input", "add-account requires root (try: sudo scryd add-account ...)")`.
- [ ] After `config_writer::upsert_account()` returns success, look up the `scryd` user's UID and GID via `nix::unistd::User::from_name("scryd")`. If the user exists (production install), `nix::unistd::chown(&config_path, Some(user.uid), Some(user.gid))`. If the user does not exist (test environment), skip the chown silently.
- [ ] After the chown step, attempt a UDS connect: build a `UdsClient` and call a no-op endpoint (e.g., `GET /accounts`). If the connect succeeds, print `apply changes: sudo systemctl restart scryd` to stdout. If it fails with `DaemonNotRunning`, print `apply changes: sudo systemctl start scryd` instead.
- [ ] Update `scryd/tests/cmd_add_account.rs` to cover the new behaviors:
  - `add_account_without_root_exits_bad_input` — invoke the binary as a non-root UID (the test runner's UID, since `cargo test` rarely runs as root). Assert exit code matches `ExitCode::BadInput` and stderr contains `requires root`.
  - `add_account_with_root_writes_config_and_prints_restart_hint` — set `SCRYD_CLI_FAKE_EUID=0` (a new test-only env var the elevation check honors when present) and run with the existing fake-UDS-server fixture from the v0.1.0 tests. Assert stdout contains `sudo systemctl restart scryd`.
  - `add_account_with_root_and_no_daemon_prints_start_hint` — same as above but no fake-UDS-server. Assert stdout contains `sudo systemctl start scryd`.
- [ ] Add the test-only `SCRYD_CLI_FAKE_EUID` env var honored by the elevation check: if set to a parseable u32, the check uses that value instead of `geteuid()`. Document inline in `main.rs` that this is test-only (gated behind `#[cfg(test)]` is not feasible because `assert_cmd` runs the release binary; instead, gate on the env var being set with a clear "test-only" comment).
- [ ] Add `nix = { version = "0.29", default-features = false, features = ["user", "fs"] }` to `scryd/Cargo.toml` if not already added by task 08.

## Acceptance criteria
- [ ] `cargo test -p scryd --test cmd_add_account` passes (all existing tests + the three new ones).
- [ ] `git grep -nE 'geteuid|SCRYD_CLI_FAKE_EUID' scryd/src/main.rs` matches the elevation check.
- [ ] `git grep -F 'nix::unistd::chown' scryd/src/main.rs` matches the post-write ownership fix.
- [ ] `git grep -F 'sudo systemctl restart scryd' scryd/src/main.rs` matches the restart-hint emission.
- [ ] `git grep -F 'sudo systemctl start scryd' scryd/src/main.rs` matches the start-hint emission.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
