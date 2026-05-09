Parent spec: [scryd v0.2.0](../../scryd-spec-v0.2.0.md)

# scryd v0.2.0 — cli

## §1 Summary

Owns the operator-visible command surface in v0.2.0: which verbs require elevation, which run as the operator's normal account, what they print on success, what they print on the new failure shapes (no elevation, daemon not running, write to a daemon-owned config), and how the CLI tells the operator to apply changes after a successful mutate. The CLI binary stays a single `scryd` executable; v0.2.0 does not introduce a setuid wrapper, a separate `scryd-admin` binary, or a daemon socket endpoint for config writes.

## §2 Codebase reconnaissance

- Existing CLI entrypoint at `scryd/src/main.rs` (388 lines, clap derive). Verbs: `add-account` (`AddAccount`), `reindex` (`Reindex`), `search` (`Search`), hidden `serve` (`Serve`). `run_serve` (line 91-97) currently exits with the deferred-integration message — task 16 is unfinished.
- Existing config-write path at `scryd/src/config_writer.rs:44-74`. `upsert_account(path, entry)` reads the TOML with `toml_edit`, mutates an `[[accounts]]` array, writes atomically via `write_atomic` (line 132-145) at mode 0600. Today this assumes the calling process can write the path.
- Existing config-path resolution at `scryd/src/main.rs:361-377`. `resolve_config_path()` looks at `XDG_CONFIG_HOME` first, then `HOME/.config`, returning `<base>/scryd/config.toml`. With `XDG_CONFIG_HOME=/etc/scryd` set in the daemon's environment, the same logic resolves to `/etc/scryd/config.toml` for the CLI when it runs as root via sudo with the env preserved — but sudo by default does NOT preserve `XDG_CONFIG_HOME`, so the CLI under sudo falls through to `HOME/.config/scryd/config.toml` which is the wrong path under v0.2.0.
- Existing exit-code conventions at `scryd/src/exit.rs`. `ExitCode::Error`, `BadInput`, `DaemonNotRunning`, `DaemonRejected`, `ConfigError`, `Other`. `bail()` (used throughout `main.rs`) emits a category prefix on stderr and exits.
- Existing UDS client at `scryd/src/uds_client.rs`. `UdsClient::from_env()` resolves the socket path from `XDG_RUNTIME_DIR/scryd/scryd.sock`, again env-driven; same sudo-and-env caveat as the config path.
- Existing search rendering at `scryd/src/output.rs`. TTY vs JSON, ANSI bolding when `is_terminal::IsTerminal::is_terminal(&stdout)` returns true. Out of scope for this slice.
- Existing rotate-password story: there is no `scryd rotate-password` verb today. v0.1.0 told operators to re-run `scryd add-account` to overwrite. v0.2.0 spec wants this as a distinct, named operation.
- Existing remove-account story: same — no verb today; spec wants it.

## §3 Decisions

1. **Elevation check pattern.** Options: per-verb `euid == 0` check in the verb handler, clap `#[command(...)]` precondition, dedicated setuid wrapper. **Chosen:** per-verb `euid == 0` check at the start of mutating handlers. Rationale: simplest; clap doesn't natively model "this verb needs root"; setuid wrappers expand the security surface.
2. **Verbs that require elevation.** Options: configure-account / rotate-password / remove-account; plus add-account as a synonym; or also reindex. **Chosen:** four mutating verbs require elevation: `add-account`, `rotate-password`, `remove-account`, plus a renamed `configure-account` aliasing `add-account` for spec-readability. Read-only verbs (`search`, `reindex`, `status` if shipped) require no elevation. Rationale: matches the spec's scope §3 In split between read-only and config-mutating.
3. **`add-account` vs `configure-account` naming.** Options: keep `add-account` only, rename to `configure-account`, ship both as aliases. **Chosen:** keep `add-account` as the canonical name; spec's "configure-account command" is the same verb in operator terms. Rationale: don't break v0.1.0 muscle memory; spec language and CLI language can differ where the spec is being abstract.
4. **Config-path resolution under sudo.** Options: rely on sudo preserving `XDG_CONFIG_HOME` (not default), have install.sh write a `/etc/sudoers.d/scryd` snippet preserving the env, hardcode `/etc/scryd/config.toml` as a fallback when running as root, ask for `--config` flag. **Chosen:** when the CLI runs as root and `XDG_CONFIG_HOME` is unset, fall back to `/etc/scryd/config.toml` (a constant in the CLI binary). When run as the operator (no elevation), continue to resolve via `XDG_CONFIG_HOME`/`HOME` for v0.1.0 compatibility on read-only verbs that read account metadata. Rationale: avoids touching sudoers; the CLI already has a path-resolution helper to extend.
5. **rotate-password verb shape.** Options: positional account_id arg + interactive password prompt; flag-based `--account-id` `--password-stdin`; both. **Chosen:** positional `<account-id>` + interactive `rpassword` prompt by default, with `--password-stdin` for scripted use. Rationale: matches the existing `add-account` shape so operators don't have to learn a different prompt flow.
6. **remove-account verb shape.** Options: positional `<account-id>` only, with `--yes` to skip a confirmation prompt; positional + always-confirm. **Chosen:** positional `<account-id>` + `--yes` to skip the confirmation. Rationale: removing accounts deletes their indexed messages on next sync; defaulting to confirm-first prevents typos.
7. **Reload-after-mutate flow.** Options: print `sudo systemctl restart scryd` hint, auto-run the restart, send SIGHUP, hit `/internal/reconcile` over the socket. **Chosen:** print the restart hint exactly. Rationale: build-and-packaging slice §3 Decision 9 already settled this; the daemon doesn't handle SIGHUP (task 16); auto-restart shells out fragilely.
8. **Daemon-not-running detection on mutate verbs.** Options: silently skip the reload hint when daemon is down, print "the daemon is not running yet — start it with `sudo systemctl start scryd`", error out. **Chosen:** print a different one-liner for daemon-down: "configuration saved; start the daemon with `sudo systemctl start scryd`". Rationale: spec failure-mode "configure-account-with-daemon-down" requires the command to succeed and start (or hint at starting) the daemon; printing the start hint instead of the restart hint matches.
9. **Status verb.** Options: ship in v0.2.0 (`scryd status` returns daemon health summary), defer. **Chosen:** defer. Rationale: spec mentions "view daemon status" as a read-only CLI verb but the actual implementation depends on task 16 (runtime serve orchestration) for the underlying health endpoint; out-of-scope until that lands.

## §4 Contracts & shapes

### CLI verbs after v0.2.0

| Verb | Elevation | Reads | Writes | Talks to daemon |
|---|---|---|---|---|
| `add-account` (a.k.a. configure-account) | required | `/etc/scryd/config.toml` | `/etc/scryd/config.toml` | no |
| `rotate-password <account-id>` | required | `/etc/scryd/config.toml` | `/etc/scryd/config.toml` | no |
| `remove-account <account-id>` | required | `/etc/scryd/config.toml` | `/etc/scryd/config.toml` | no |
| `search <query>` | none | n/a | n/a | yes (UDS) |
| `reindex` | none | n/a | n/a | yes (UDS) |
| `serve` (hidden) | invoked by systemd, not by operators | env-driven | n/a | n/a |

### `add-account` flag set (unchanged from v0.1.0)

- `--account-id <regex `[a-z0-9_-]+`>` — required if non-interactive
- `--host <hostname>` — required if non-interactive
- `--port <u16>` — defaults to 993
- `--user <imap-username>` — required if non-interactive
- `--password-stdin` — read password from stdin instead of `rpassword` prompt
- `--folders <CSV>` — defaults to `INBOX` after the prompt's blank-line confirmation

Without flags, the CLI prompts interactively for each missing field. Same UX as v0.1.0; the only change in v0.2.0 is the elevation precondition.

### `rotate-password` flag set (new in v0.2.0)

- positional `<account-id>` — required
- `--password-stdin` — read new password from stdin instead of `rpassword` prompt

Looks up `<account-id>` in `/etc/scryd/config.toml`, replaces the password field via `toml_edit`, writes atomically. Preserves operator comments and field ordering (the existing `upsert_account` already handles this; `rotate_password` reuses the same `toml_edit`-based write path with a smaller mutation).

### `remove-account` flag set (new in v0.2.0)

- positional `<account-id>` — required
- `--yes` — skip the confirmation prompt

Looks up `<account-id>`, prints "remove account `<account-id>` from /etc/scryd/config.toml? [y/N]" unless `--yes`, then deletes the table from the TOML and writes atomically. The daemon's resync after restart drops the orphaned messages from the search corpus.

### Failure messages and exit codes

- No elevation on a mutating verb: stderr `scryd: <verb> requires root (try: sudo scryd <verb> ...)`, exit `BadInput` (2).
- Daemon not running on a read-only verb: stderr `scryd: daemon-not-running: scryd is not running. Start it with: sudo systemctl start scryd`, exit `DaemonNotRunning` (2).
- Config not present on a mutating verb: stderr `scryd: config-error: /etc/scryd/config.toml is missing — has install.sh been run?`, exit `ConfigError` (3).
- Config write success on a mutating verb: stdout `account '<id>' saved` (or `password rotated for '<id>'`, `account '<id>' removed`), then `apply changes: sudo systemctl restart scryd`. If the daemon was not running when the write happened: `apply changes: sudo systemctl start scryd` instead.
- Account not found on rotate/remove: stderr `scryd: bad-input: account '<id>' is not configured`, exit `BadInput` (2).

### Path resolution rules

The CLI resolves the config path with this precedence:

1. If `XDG_CONFIG_HOME` is set and non-empty: `${XDG_CONFIG_HOME}/scryd/config.toml`.
2. Else if `EUID == 0` and `XDG_CONFIG_HOME` is unset: `/etc/scryd/config.toml` (the v0.2.0 daemon-owned constant).
3. Else if `HOME` is set: `${HOME}/.config/scryd/config.toml` (the v0.1.0 fallback for read-only verbs).
4. Else: error.

This keeps v0.1.0 read-only flows working on hosts with v0.1.0 binaries still installed and gives v0.2.0 mutating flows the right path under sudo without needing sudoers tweaks.

## §5 Sequence

1. **Operator runs `sudo scryd add-account` interactively.** clap parses, `run_add_account` checks `EUID == 0`. Pass.
2. **CLI prompts for missing fields.** account-id (validated against `^[a-z0-9_-]+$`), host, port, user, password (rpassword), folders.
3. **CLI resolves config path.** `EUID == 0` and `XDG_CONFIG_HOME` unset → `/etc/scryd/config.toml`.
4. **CLI calls `config_writer::upsert_account`.** Reads existing TOML (empty after fresh install), inserts the new `[[accounts]]` table, writes atomically. The atomic write happens as root, so the resulting file is owned `root:root` mode 0600 — install.sh's chown to `scryd:scryd` does not get re-applied.
5. **CLI fixes ownership.** After the write, the CLI calls `chown scryd:scryd /etc/scryd/config.toml` (Unix `std::os::unix::fs::chown` via the `nix` crate or a direct libc call). On non-root or non-Unix this is a no-op.
6. **CLI checks daemon status.** Connects to the UDS at `XDG_RUNTIME_DIR=/run/scryd/scryd.sock` (the daemon's env, not the operator's; sudo doesn't preserve `XDG_RUNTIME_DIR` so the CLI also has the same `EUID==0` fallback to `/run/scryd/scryd.sock`). Connection success → daemon running → print restart hint. Connection refused / path missing → daemon not running → print start hint.
7. **CLI exits 0.** Operator runs the printed hint.
8. **Operator runs `scryd search "foo"` from their normal account.** No elevation check. CLI resolves `XDG_RUNTIME_DIR` from the operator's env (or `/run/scryd/scryd.sock` if neither env nor user-specific path applies — at this point the operator's session has the standard `XDG_RUNTIME_DIR=/run/user/<uid>/`, which does NOT contain `scryd/scryd.sock`).
9. **Path mismatch on operator side.** The operator's session's `XDG_RUNTIME_DIR` resolves to `/run/user/<operator-uid>/scryd/scryd.sock`, which does not exist. The CLI's connection attempt fails. The CLI prints `scryd: daemon-not-running: ...` even though the daemon is alive at `/run/scryd/scryd.sock`.

This is a real problem the slice has to solve. **Chosen path resolution for the runtime dir** (added as a sub-rule of Decision 4): on Linux, the CLI tries `$XDG_RUNTIME_DIR/scryd/scryd.sock` first, then falls back to `/run/scryd/scryd.sock`, in that order. The operator's session-level XDG runtime dir (under `/run/user/`) is irrelevant for v0.2.0 because the daemon's socket is under the system-level `/run/scryd/`. The fallback ordering means v0.1.0 per-user installs (where the socket lives under `/run/user/<uid>/scryd/`) keep working.

This sub-rule is recorded here, not in build-and-packaging, because the runtime-dir behavior is a CLI concern.

## §6 Out of scope

- The peercred-side acceptance — owned by the security slice.
- Install-time creation of `/etc/scryd/config.toml` and chowning to `scryd:scryd` — owned by build-and-packaging. The CLI's chown after a write is a different concern (re-applying ownership after `toml_edit`'s atomic write briefly drops it).
- A daemon-side socket endpoint for config mutations (which would let `scryd add-account` work without sudo). Spec §3 Out: "Authentication of local programs beyond the peer-account check (no API tokens, no OAuth or session protocol between operator and daemon)" — adding a write-the-config endpoint would either reintroduce the peer-account-can-write-anyone's-account problem or require per-peer attribution. Either way, out of v0.2.0.
- A `status` verb. Pending task 16's runtime-serve orchestration.
- Auto-completion (bash/zsh/fish). v0.1.0 cli slice already deferred this; same here.

## §7 Open questions

- Whether `rotate-password` should send the running daemon a poke (e.g., touch a sentinel file the daemon's IMAP supervisor watches) so the new credential is picked up faster than waiting for the next sync attempt's natural retry. Default position: rely on the operator running `sudo systemctl restart scryd` after rotate, identical to add-account; the spec's rotate criterion permits a sync-cycle delay.
- Whether the `chown scryd:scryd` after config write requires a new Cargo dependency (`nix`) or can be done via a direct `libc::chown` call. Default position: the workspace already has `nix` in `crates/scryd-imap`'s dep graph indirectly via `tokio`; if not, prefer adding `nix` over calling libc by hand.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
