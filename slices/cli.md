Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — cli

## §1 Summary

Owns the operator-facing command-line surface — exactly three verbs (`add-account`, `reindex`, `search`) plus a hidden `serve` verb the systemd user unit invokes — argument parsing, interactive credential capture, atomic config-file writes, and dispatch to the running daemon over the Unix socket. Also pins exit-code conventions, output formatting, and the human-readable error strings that show up at a terminal.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External Rust crates this slice leans on (versions pinned at coding time, in build-and-packaging):

- `clap` (derive feature) — argument parsing, help generation.
- `rpassword` — read a password without echoing to the terminal.
- `toml_edit` — read-modify-write `config.toml` while preserving comments and ordering when an existing file is updated.
- `reqwest` (with `unix-socket` feature) or `hyper` directly — HTTP client speaking over a Unix domain socket. Decision 6 picks.
- `serde_json` — parse API responses for non-`--json` rendering.

## §3 Decisions

1. **Single binary, four verbs.** `scryd add-account`, `scryd reindex`, `scryd search`, plus a hidden `scryd serve`. `--help` lists only the three operator verbs; `serve` exists for `systemd --user`'s `ExecStart=` and is documented in build-and-packaging only. Rationale: the spec's operator-visible CLI is exactly three verbs; the daemon entrypoint has to live somewhere; making it hidden keeps the operator's mental model clean.
2. **No `--instance`, `--socket`, `--port`, `--host` flags. Anywhere.** The CLI always resolves the socket via `$XDG_RUNTIME_DIR/scryd/scryd.sock` (multi-instance-isolation slice). Rationale: the multi-instance-isolation acceptance criterion that "the CLI cannot be aimed at another operator's instance" is enforced by the absence of any redirection flag, not by validation.
3. **`add-account` writes the config file directly.** Same uid as the daemon → CLI has filesystem access. After writing, CLI dispatches `POST /internal/reconcile` to the running daemon (if any). If the daemon isn't running, the CLI prints a hint to start it and exits 0 — config write succeeded; no-op on the reload is correct. Rationale: pushing the write through the daemon would require the daemon to do file IO for the operator's local concern; direct write keeps the daemon ignorant of `add-account` semantics.
4. **`add-account` is interactive by default; non-interactive via flags.** Without flags, prompts for: account id, IMAP host, port (default 993), username, app password (no echo via `rpassword`), folders (comma-separated, default `INBOX`). With `--account-id`, `--host`, `--port`, `--user`, `--password-stdin`, `--folders` flags, runs non-interactively. Rationale: interactive matches the spec's "prompts for host, user, app password" promise; flags are the path for ops automation that wants to seed config from a secret manager.
5. **`add-account` against an existing account-id rotates the password.** Operator passes the same id → CLI reads the existing entry, replaces only the fields the operator supplied (or re-prompts for password by default), writes back. The daemon's `reconcile` cascades to imap-sync's per-account supervisor, which closes connections and re-opens with the new password without losing UID-validity state (per imap-sync slice §3 Decision 8). Rationale: spec promises rotation without index loss, and the spec says `add-account` doubles as rotation.
6. **HTTP-over-UDS client.** `reqwest` with the `unix-socket` feature. Rationale: cleanest story for streaming responses (raw .eml from `/message/:id/raw` if cli ever surfaces it), best-known async API, smallest code in the cli crate.
7. **Atomic config write.** Read existing config (or start blank). Apply the mutation in memory via `toml_edit` (preserves operator's comments and key ordering on rotation). Write to `<config-path>.tmp` mode `0600`, `fsync`, `rename(2)` over `<config-path>`. Rationale: a concurrent crash never leaves a half-written `config.toml`; daemon-side startup self-checks (multi-instance-isolation §3 Decision 9) verify mode `0600` after the rename.
8. **`search` output formatting.** Default: a plain-text two-line block per hit — line 1 `[<account_id>] <date-iso> <sender_name> <<addr>> · <subject>`; line 2 indented snippet with markdown bold rendered as ANSI bold (when stdout is a TTY) or stripped to plain text (when piped). Trailing line with `<count> hits in <ms>ms`. With `--json`, dump the API JSON response unmodified. Rationale: shell-friendly default, machine-friendly with the flag, no opinionated table layout that breaks on narrow terminals.
9. **`search` filter flags.** `--from <substr>`, `--since <YYYY-MM-DD>`, `--until <YYYY-MM-DD>`, `--folder <name>`, `--account <id>`, `--limit <1..200>` (default 20), `--mode <fulltext|semantic|hybrid>` (default `fulltext`), `--json`. Plus the positional `<query>` (free-form string; quote in shell). Rationale: 1:1 mapping to the api slice's `GET /search` parameters; nothing more.
10. **`reindex` is fire-and-forget.** Dispatches `POST /internal/reindex` and prints `reindex started; search continues to serve during rebuild · check 'journalctl --user -u scryd' for progress`. Exits 0 immediately on 202. On 409 (already in progress), prints `a reindex is already running` and exits 0. Rationale: api slice returns 202; v1 has no progress endpoint; tying the CLI's exit to "rebuild done" would make it noticeably slow on large corpora.
11. **Closed exit-code set.**
    - `0` — verb succeeded.
    - `1` — generic error (network blip on the local socket, JSON parse failure, etc.).
    - `2` — `daemon_not_running` (CLI tried to dispatch and got `ENOENT`/`ECONNREFUSED` on the socket).
    - `3` — `config_error` (config file unreadable, malformed, permission wrong).
    - `4` — `bad_input` (operator typed an invalid date, mode, port, or empty required field on a non-interactive run).
    - `5` — `daemon_rejected` (api returned 4xx; the body's error code is printed and exit code 5 is returned for any 4xx outside `daemon_not_running`).
    Rationale: small closed set; scriptable; distinct codes for the three "operator's fault" vs "daemon's fault" vs "they need to start the daemon" cases.
12. **Hidden `serve` verb.** `scryd serve` runs the daemon in the foreground attached to the current stdin/stdout/stderr — no double-fork, no PID-file. systemd's `Type=simple` (build-and-packaging slice) supervises it. With `--help`, `serve` does not appear; with `--help serve`, it does. Rationale: cleanest interaction with systemd's process model; the operator never invokes `serve` by hand on a deployed install.
13. **No tab completion script generation in v1.** clap can produce one but it's a packaging concern; if shipped, it's via build-and-packaging, not by a `scryd completions <shell>` verb that would inflate the operator's verb list. Rationale: spec's three-verb constraint.

## §4 Contracts & shapes

Internal Rust crate (provisional): `scryd-cli`. The binary that ships is built from this crate; it links into `scryd-daemon` (or whatever build-and-packaging names the daemon crate) for the `serve` path.

`clap` command tree:

```
scryd
├── add-account [--account-id <id>] [--host <host>] [--port <n>] [--user <user>]
│              [--password-stdin] [--folders <comma-list>]
├── reindex
├── search <query> [--from <s>] [--since <date>] [--until <date>] [--folder <name>]
│         [--account <id>] [--limit <n>] [--mode <fulltext|semantic|hybrid>] [--json]
└── serve              (hidden)
```

`add-account` interactive prompts (Decision 4):

```
account id: <freeform string, must match [a-z0-9_-]+, default "primary" if no accounts exist>
imap host: <hostname>
imap port [993]: <n>
username: <string>
app password: <hidden>
folders [INBOX]: <comma-separated list>
```

Validation: account-id matches `^[a-z0-9_-]+$`; port in `1..=65535`; folders is a non-empty list.

`add-account` writes a `[[accounts]]` block to `$XDG_CONFIG_HOME/scryd/config.toml`. Existing block with the same `id` is replaced in place, preserving the file's other content and comments. The full config schema:

```toml
[sync]
poll_interval_seconds = 300
use_idle              = true
folders               = ["INBOX"]   # default per-account override

[indexers]
semantic = true                     # set false to skip Witchcraft embedding pipeline

[[accounts]]
id       = "primary"
host     = "imap.example.com"
port     = 993
user     = "user@example.com"
password = "<app password>"
folders  = ["INBOX"]                # optional override
```

`search` output format (default, TTY):

```
[primary] 2026-04-30 Alice <alice@example.com> · Re: invoice March
    …the **invoice** is attached, please review and let me know if anything…
[primary] 2026-04-22 Acme Billing <bill@acme.example> · April invoice
    …your monthly **invoice** is now available…

2 hits in 18ms
```

When piped (stdout is not a TTY), bold rendering strips back to literal `**…**`. With `--json`, the api response body is printed verbatim and no trailing summary line is emitted.

`reindex` output (success):

```
reindex started; search continues to serve during rebuild
check progress with: journalctl --user -u scryd
```

`reindex` output (already running):

```
a reindex is already running
```

`add-account` output (success, daemon running):

```
account 'primary' saved to ~/.config/scryd/config.toml
daemon reloaded; account is now syncing
```

`add-account` output (success, daemon not running):

```
account 'primary' saved to ~/.config/scryd/config.toml
start the daemon: systemctl --user start scryd
```

Error rendering (any exit code ≥ 1) writes to stderr:

```
scryd: <category>: <human-readable reason>
```

…where `<category>` is one of `error`, `daemon-not-running`, `config-error`, `bad-input`, `daemon-rejected`.

Constants:

- `DEFAULT_PORT = 993`
- `DEFAULT_FOLDERS = ["INBOX"]`
- `ACCOUNT_ID_PATTERN = "^[a-z0-9_-]+$"`
- `LIMIT_DEFAULT = 20` (must match api slice's value)
- `TTY_BOLD_OPEN = "\x1b[1m"`, `TTY_BOLD_CLOSE = "\x1b[22m"`

## §5 Sequence

1. **`scryd add-account`** (interactive). Parse args → if any flag missing, prompt → validate inputs → read existing `config.toml` (or start blank) → upsert the `[[accounts]]` block via `toml_edit` → write to `config.toml.tmp` mode `0600` → `fsync` → `rename` → attempt `POST /internal/reconcile` over UDS. If reconcile succeeds (202): print "daemon reloaded" success message, exit 0. If reconcile gets `ENOENT`/`ECONNREFUSED`: print "start the daemon" message, exit 0. If reconcile returns 4xx/5xx: print the error body, exit 5.
2. **`scryd add-account --account-id A --host H ...`** (non-interactive). Same as 1 but no prompts; missing required fields → print bad-input error, exit 4.
3. **`scryd reindex`.** Dispatch `POST /internal/reindex` over UDS → 202: print fire-and-forget message, exit 0. 409: print "already running", exit 0. ENOENT/ECONNREFUSED: print daemon-not-running, exit 2.
4. **`scryd search <query> [...flags]`.** Construct the URL `/search?q=<query>&...` → `GET` over UDS → 200: parse JSON → format per Decision 8 → exit 0. 400: print bad-query body, exit 5. 503: print shutting-down, exit 5. ENOENT/ECONNREFUSED: print daemon-not-running, exit 2.
5. **`scryd serve`** (hidden). Initializes the daemon as build-and-packaging dictates: parse `config.toml`, run multi-instance-isolation startup self-checks, open `meta.sqlite`, run migrations, mount api routes onto the UDS, spawn imap-sync scheduler and indexer, hand off to tokio runtime. Stays in foreground; SIGTERM → graceful shutdown.
6. **CLI startup, any verb.** Resolve `$XDG_RUNTIME_DIR`. If unset, print "scryd: error: XDG_RUNTIME_DIR is not set" and exit 1 (matches what the daemon would emit on startup; CLI never has any other place to talk to). For `add-account`, also resolve `$XDG_CONFIG_HOME` (defaulting to `$HOME/.config`).
7. **Bold-rendering decision.** On startup, the search verb checks `isatty(stdout)`. TTY → render `**…**` snippet markup as ANSI bold; non-TTY → leave literal `**…**`. `--json` short-circuits the renderer entirely.
8. **Daemon-not-running detection.** A connect error of `ENOENT` (socket file absent) or `ECONNREFUSED` (file present, no listener) maps to exit code `2` and the human message `scryd: daemon-not-running: scryd is not running for user $USER. Start it with: systemctl --user start scryd`. Other connect errors map to exit code `1`.

## §6 Out of scope

- The HTTP request/response shapes themselves (api slice).
- The systemd user unit file and how `serve` is invoked by it (build-and-packaging slice).
- The Unix socket lifecycle and OS-identity check (multi-instance-isolation slice).
- Reading the password from a system keyring or secret manager. v1: app password sits in `config.toml` mode `0600`. Keyring integration is a v2 ergonomic.
- A `scryd status` verb. Spec is explicit: status is via system logs, not a CLI verb.
- A `scryd list-accounts` verb. Spec is explicit: not in the CLI surface; the `GET /accounts` API endpoint covers programmatic listing.
- Pagination of `search` output. Default `limit` is 20; operators wanting more pass `--limit`.
- Tab completion shipping logistics (build-and-packaging slice).
- Logging from the CLI itself. The CLI is short-lived; errors go to stderr, that's the entire telemetry surface.

## §7 Open questions

- Whether `add-account` should also support `--password-from-keyring <key>` to pull the password from `secret-tool` / libsecret instead of stdin or a prompt. v1 says no; the operator can always do `cat <<<"<pass>" | scryd add-account --password-stdin`. Confirm v1 deferral.
- Whether `reindex` should accept a confirmation flag (`--yes` to skip a "this will rebuild your search index, continue? [y/N]" prompt). v1: no prompt — a CLI-only operator action whose worst case is "search returns smaller result counts for a few minutes". Confirm.
- Whether the search output should include `score` next to each hit. v1: no, score is only useful for debugging ranking; render via `--json` if needed.
- Whether `scryd search` should also be invocable as `scryd <query>` (no `search` subverb). v1: no — clap-style requires the subverb; cleaner mental model.
- Whether the cli should emit a one-time onboarding hint after `add-account` on a fresh install (e.g., point operators at `loginctl enable-linger` for boot-survival). v1: yes if `loginctl show-user $USER` reports `Linger=no`. Confirm before coding.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
