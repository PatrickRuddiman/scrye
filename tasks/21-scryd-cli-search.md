Parent slice: [cli](../slices/cli.md)
Depends on: 20, 18

# Task 21 — scryd-cli-search

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement `scryd search` end-to-end: clap flags, dispatch to `GET /search`, two-line per-hit plain-text output with TTY-aware ANSI bold rendering, and `--json` passthrough.

## Tasks
- [ ] In `scryd/src/cmd_search.rs`, define the clap args: positional `<query>` (required, may be empty string for "most recent"); flags `--from <substr>`, `--since <YYYY-MM-DD>`, `--until <YYYY-MM-DD>`, `--folder <name>`, `--account <id>`, `--limit <n>` (default 20), `--mode <fulltext|semantic|hybrid>` (default `fulltext`), `--json`.
- [ ] Implement `pub async fn run(args: SearchArgs) -> Result<(), CliError>`: build a `UdsClient` via `uds_client::new()` → URL-encode query parameters → call `client.get_json("/search?...")` → if `--json`, print the JSON body verbatim and exit 0 → else format per Decision 8.
- [ ] In `scryd/src/output.rs`, expose `pub fn render_search(resp: &SearchResponseDto, tty: bool) -> String`. Each hit produces two lines per cli slice §4: line 1 `[<account>] <YYYY-MM-DD> <name> <<addr>> · <subject>`; line 2 indented snippet with `**…**` rendered as ANSI bold (`\x1b[1m…\x1b[22m`) when `tty == true`, or left literal otherwise. Trailing summary line `<n> hits in <elapsed_ms>ms`.
- [ ] Use `is_terminal::IsTerminal` against stdout to decide TTY mode at runtime; do not couple to env vars.
- [ ] Map api errors per Decision 11: `bad_query` → exit code 5 with stderr `scryd: daemon-rejected: <message>`; `daemon_not_running` (from the UDS layer) → exit code 2; any 5xx → exit code 5 with `daemon-rejected: internal_error`.
- [ ] Write integration tests in `scryd/tests/cmd_search.rs` using `assert_cmd` and a tokio test server that mimics `GET /search`: stage a fake api on a temp UDS that returns a fixture JSON with two hits → invoke `scryd search invoice --limit 2` with `XDG_RUNTIME_DIR` pointing at the temp dir → assert stdout matches a regex of two `[primary]` blocks plus the summary line; invoke with `--json` → assert stdout is exactly the fixture JSON; invoke with `--mode bogus` → exit code 1 (clap parse error before dispatch).
- [ ] Write a unit test for `render_search` against TTY=true and TTY=false: TTY rendering wraps `**foo**` as `\x1b[1mfoo\x1b[22m`; non-TTY leaves `**foo**` literal.

## Acceptance criteria
- [ ] `cargo test -p scryd --test cmd_search` passes.
- [ ] `cargo build --release -p scryd` exits 0.
- [ ] `target/release/scryd search --help 2>&1 | grep -E '\-\-mode' | grep -E 'fulltext|semantic|hybrid' | wc -l` outputs at least 1.
- [ ] `git grep -nE '\\\\x1b\[1m|\\u\{001b\}\[1m' scryd/src/output.rs` matches the ANSI bold open sequence.
- [ ] `git grep -nE 'is_terminal' scryd/src/output.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
