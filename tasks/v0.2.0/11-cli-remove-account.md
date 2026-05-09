Parent slice: [scryd v0.2.0 — cli](../../slices/0.2.0/cli.md)
Depends on: 09

# Task 11 — cli-remove-account

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Ship `scryd remove-account <account-id>` verb that removes an `[[accounts]]` entry from `/etc/scryd/config.toml`, requiring root, defaulting to a y/N confirmation prompt that `--yes` skips, chowning the result and printing the restart hint.

## Tasks
- [x] In `scryd/src/main.rs` `Verb` enum, add `RemoveAccount(RemoveAccountArgs)` variant with `#[command(name = "remove-account", about = "Remove a configured IMAP account")]`.
- [x] Define `RemoveAccountArgs`: positional `account_id: String` (required), `--yes` boolean flag.
- [x] Add `Verb::RemoveAccount` arm in `main()` calling `run_remove_account(args)`.
- [x] Implement `run_remove_account` in `scryd/src/main.rs`:
  - Elevation precondition (identical pattern to `run_add_account`).
  - Validate `account_id` against `^[a-z0-9_-]+$`.
  - Resolve config path.
  - Look up the account via a read-only walk of the TOML (`config_writer::find_account(path, account_id)` — new helper or inline read). Bail `BadInput` if not found.
  - Unless `args.yes`: prompt `remove account '<id>' from /etc/scryd/config.toml? [y/N]:` on stderr, read one line from stdin. Accept `y`/`Y`/`yes` only. Anything else exits 0 with stdout `aborted`.
  - Call `config_writer::remove_account(path, account_id)`.
  - Chown to `scryd:scryd`.
  - Print `account '<id>' removed`.
  - Probe daemon and print restart-vs-start hint.
- [x] Add `pub fn remove_account(path: &Path, account_id: &str) -> Result<(), ConfigWriterError>` to `scryd/src/config_writer.rs`. Walks the `toml_edit::DocumentMut`'s `accounts` array, finds the matching entry, removes it, writes atomically. Returns `AccountNotFound` (already added in task 10) if the entry wasn't there.
- [x] Create `scryd/tests/cmd_remove_account.rs`. Tests: `remove_account_without_root_exits_bad_input`, `remove_account_for_unknown_account_exits_bad_input`, `remove_account_with_yes_flag_skips_prompt_and_removes` (fixture config with two accounts, remove one with `--yes`, assert the other survives byte-for-byte), `remove_account_without_yes_flag_aborts_on_blank_input` (write `\n` to stdin, assert exit 0 stdout `aborted`, assert config unchanged), `remove_account_help_lists_yes_flag` (`scryd remove-account --help` shows `--yes`).

## Acceptance criteria
- [x] `cargo test -p scryd --test cmd_remove_account` passes (five new tests).
- [x] `cargo build --release -p scryd` exits 0.
- [x] `target/release/scryd remove-account --help 2>&1 | grep -E 'remove-account'` matches.
- [x] `target/release/scryd remove-account --help 2>&1 | grep -F '--yes'` matches.
- [x] `git grep -n 'RemoveAccount' scryd/src/main.rs` matches the enum variant + dispatch.
- [x] `git grep -n 'pub fn remove_account' scryd/src/config_writer.rs` matches the new helper.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
