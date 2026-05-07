Parent slice: [cli](../slices/cli.md)
Depends on: 20, 19

# Task 22 — scryd-cli-add-account

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `scryd add-account` interactive + non-interactive: clap flags, hidden-input password prompt, atomic `toml_edit`-based config write, and reconcile dispatch when the daemon is running.

## Tasks
- [ ] In `scryd/src/cmd_add_account.rs`, define the clap args: optional flags `--account-id`, `--host`, `--port` (default 993), `--user`, `--password-stdin`, `--folders` (comma-separated, default `INBOX`).
- [ ] Implement the interactive prompt path when flags are missing: prompt for each missing field via `std::io::Stdin`; for the password, use `rpassword::prompt_password("app password: ")`. Validate `account_id` matches `^[a-z0-9_-]+$`; `port` is `1..=65535`; `folders` is non-empty.
- [ ] Implement the non-interactive flag path: every required field present → no prompt; `--password-stdin` reads the password from stdin (line-trimmed); fields missing → exit code 4 (`bad_input`) with a clear error.
- [ ] In `scryd/src/config_writer.rs`, expose `pub fn upsert_account(config_path: &Path, entry: AccountEntry) -> Result<(), CliError>`. Use `toml_edit::DocumentMut` to load existing config (or create blank), find any existing `[[accounts]]` block with the same `id`, replace its scalar fields in place (preserves comments / ordering), or append a new block at the end if not found. Write to `<config_path>.tmp` mode `0600`, `fsync`, `rename` over `<config_path>`.
- [ ] If `$XDG_CONFIG_HOME/scryd/` does not exist, create it mode `0700` before the write.
- [ ] After the write succeeds, attempt `POST /internal/reconcile` via `UdsClient`. On 202 → print `account '<id>' saved to ~/.config/scryd/config.toml` then `daemon reloaded; account is now syncing`. On `daemon_not_running` → print `account '<id>' saved …` then `start the daemon: systemctl --user start scryd`. On 4xx/5xx → print the saved-to message but exit 5 with the daemon's error.
- [ ] After a successful save+reconcile, if `loginctl show-user $USER` reports `Linger=no` (use `Command::new("loginctl").args(["show-user", "$USER"]).output()`), print a one-time hint suggesting `loginctl enable-linger`. Per cli slice §7 last item.
- [ ] Write integration tests in `scryd/tests/cmd_add_account.rs` using `assert_cmd` + `tempfile`: 
  - non-interactive flag path with all flags + `--password-stdin` writes a valid `config.toml`; the file mode is `0600`; the daemon-stub on a temp UDS receives `POST /internal/reconcile`.
  - interactive path is tested by piping a script of newline-separated answers via `assert_cmd`'s `write_stdin`; assert the file is created.
  - re-running `add-account` for the same id with a new password modifies the password field in place; the file's existing comments survive.
  - missing required flag in non-interactive path → exit code 4.
  - `account_id="HasUpper"` → exit code 4 (regex rejection).
- [ ] Write a unit test for `config_writer::upsert_account` asserting comment preservation (input file has `# my old comment` above `id = "primary"`; after upsert the comment is still on the line above).

## Acceptance criteria
- [ ] `cargo test -p scryd --test cmd_add_account` passes.
- [ ] `cargo build --release -p scryd` exits 0.
- [ ] `git grep -nE 'rpassword::prompt_password' scryd/src/cmd_add_account.rs` matches.
- [ ] `git grep -nE 'toml_edit' scryd/src/config_writer.rs` matches.
- [ ] `git grep -nE 'rename' scryd/src/config_writer.rs` matches the atomic-rename call.
- [ ] `git grep -nE '0o600|mode\(0o600\)' scryd/src/config_writer.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
