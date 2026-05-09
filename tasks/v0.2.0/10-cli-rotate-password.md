Parent slice: [scryd v0.2.0 — cli](../../slices/0.2.0/cli.md)
Depends on: 09

# Task 10 — cli-rotate-password

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Ship a new `scryd rotate-password <account-id>` verb that mirrors `add-account`'s elevation, prompt, write-and-chown, restart-hint flow but mutates only the `password` field of an existing account in `/etc/scryd/config.toml`, preserving the rest of the entry.

## Tasks
- [ ] In `scryd/src/main.rs` `Verb` enum, add `RotatePassword(RotatePasswordArgs)` variant with `#[command(name = "rotate-password", about = "Rotate the IMAP credential for a configured account")]`.
- [ ] Define `RotatePasswordArgs`: positional `account_id: String` (required), `--password-stdin` boolean flag.
- [ ] Add a `Verb::RotatePassword` arm in `main()` that calls `run_rotate_password(args)`.
- [ ] Implement `run_rotate_password(args: RotatePasswordArgs)` in `scryd/src/main.rs`:
  - Run the same elevation precondition as `run_add_account` (honoring `SCRYD_CLI_FAKE_EUID`).
  - Validate `args.account_id` against `^[a-z0-9_-]+$` regex; bail `BadInput` on mismatch.
  - Read password: if `--password-stdin`, read from stdin via `io::stdin().lock().read_to_string`; else `rpassword::prompt_password("new app password: ")`. Bail `BadInput` if empty.
  - Resolve config path via `resolve_config_path()` (already updated by task 08 for sudo-EUID==0 → `/etc/scryd/config.toml`).
  - Call new `config_writer::rotate_password(path, account_id, new_password)` (see next bullet).
  - Chown the resulting file to `scryd:scryd` (skipping silently if the user doesn't exist).
  - Print `password rotated for '<id>'` to stdout.
  - Probe the daemon's UDS and print the restart-vs-start hint, identical to `add-account`.
- [ ] Add `pub fn rotate_password(path: &Path, account_id: &str, new_password: &str) -> Result<(), ConfigWriterError>` to `scryd/src/config_writer.rs`. Implementation: read the TOML via `toml_edit::DocumentMut`, walk the `accounts` array-of-tables, find the entry whose `id == account_id`, replace its `password` value with `new_password`, write atomically via the existing `write_atomic` helper (reusing the `.tmp` + fsync + rename pattern at line 132-145). On account-not-found, return a typed `ConfigWriterError::AccountNotFound { account_id: String }` mapped to `ExitCode::BadInput` in the caller.
- [ ] Create `scryd/tests/cmd_rotate_password.rs`. Tests: `rotate_password_without_root_exits_bad_input`, `rotate_password_for_unknown_account_exits_bad_input`, `rotate_password_for_existing_account_updates_field_and_preserves_rest` (write a fixture config with two accounts, rotate one, parse the result, assert the password changed AND the other account's fields are byte-identical), `rotate_password_help_lists_only_the_verb_no_extra_flags` (`scryd rotate-password --help` mentions positional `<account-id>` and `--password-stdin` only).

## Acceptance criteria
- [ ] `cargo test -p scryd --test cmd_rotate_password` passes (four new tests).
- [ ] `cargo build --release -p scryd` exits 0.
- [ ] `target/release/scryd rotate-password --help 2>&1 | grep -E 'rotate-password'` matches.
- [ ] `git grep -n 'RotatePassword' scryd/src/main.rs` matches the enum variant + dispatch.
- [ ] `git grep -n 'pub fn rotate_password' scryd/src/config_writer.rs` matches the new helper.
- [ ] `git grep -F 'AccountNotFound' scryd/src/config_writer.rs` matches the new error variant.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
