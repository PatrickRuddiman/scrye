Parent plan: scryd v0.3.1 — service pivot
Depends on: 03

# Task 04 — drop-cli-elevation

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Remove the explicit `cli_effective_euid() != 0` bail in the three mutating CLI verbs (`add-account`, `rotate-password`, `remove-account`). Mutations still need write access to `/etc/scryd/config.toml` (operator runs `sudo scryd add-account` like editing any other system config), but the security guard goes away — it was never a security primitive, just a safety net for the v0.2.0 isolation model that no longer applies.

## Tasks
- [x] In `scryd/src/main.rs:265-269` (`run_add_account`), delete the `if cli_effective_euid() != 0 { bail(...) }` block.
- [x] In `scryd/src/main.rs:348-352` (`run_rotate_password`), delete the same block.
- [x] In `scryd/src/main.rs:393-397` (`run_remove_account`), delete the same block.
- [x] In `scryd/src/main.rs:444-451` (`cli_effective_euid`), keep the helper — it's used by `chown_to_scryd` for an EUID == 0 check below, and harmless either way.
- [x] In `scryd/src/main.rs:454-460` (`chown_to_scryd`), make it a best-effort no-op when not running as root — the existing `if let Ok(Some(user)) = nix::unistd::User::from_name("scryd")` already silently skips when the scryd user doesn't exist; extend the early-return to also skip when `geteuid() != 0` since chown without root would fail anyway. Log via `tracing::debug!` rather than panicking or surfacing an error.
- [x] Update `scryd/tests/cmd_add_account.rs:79-105` (`add_account_without_root_exits_bad_input`): rename to `add_account_writes_config_without_explicit_root_check`, drop the `SCRYD_CLI_FAKE_EUID="1000"` setup, assert exit 0 and that the config was written. The test still uses a non-zero fake euid; it just expects the write to succeed.
- [x] Update existing happy-path tests that set `SCRYD_CLI_FAKE_EUID="0"`: keep the env var (since chown_to_scryd reads geteuid for the early-return, and tests don't run as root); the tests no longer NEED it for the bail check, but it stays harmless.
- [x] Apply the same rename + drop pattern in `scryd/tests/cmd_rotate_password.rs::rotate_password_without_root_exits_bad_input` and `scryd/tests/cmd_remove_account.rs::remove_account_without_root_exits_bad_input`.

## Acceptance criteria
- [x] `cargo test -p scryd --test cmd_add_account --test cmd_rotate_password --test cmd_remove_account` passes (every existing test plus the renamed-and-flipped trio).
- [x] `! git grep -nE 'requires root \(try: sudo' scryd/src/main.rs` (the elevation error messages are gone).
- [x] `! git grep -F 'add_account_without_root_exits_bad_input' scryd/tests/cmd_add_account.rs` (test renamed).
- [x] `git grep -nE 'fn cli_effective_euid' scryd/src/main.rs` matches (helper kept; just no callers in the bail position).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
