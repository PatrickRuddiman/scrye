Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — multi-instance-isolation

## §1 Summary

Owns the OS-level mechanism that makes "each user installs their own instance" actually safe: the per-user IPC channel callers reach the daemon on, the per-user filesystem layout for config/data/runtime artifacts, the per-user service-supervisor model, and the auto-routing that points an operator's CLI at exactly their own instance with no configuration. Every cross-user-isolation acceptance criterion in the parent spec ultimately lands here.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External Linux primitives this slice leans on:

- XDG Base Directory Specification — `$XDG_CONFIG_HOME` (default `~/.config`), `$XDG_DATA_HOME` (default `~/.local/share`), `$XDG_RUNTIME_DIR` (set by `systemd-logind` to `/run/user/<uid>`, mode `0700`, owner = the user, lifetime = login session).
- Unix domain sockets — filesystem-permission-gated; `getsockopt(SOL_SOCKET, SO_PEERCRED)` lets a server inspect the connecting peer's uid/gid/pid.
- `systemd --user` — per-user service manager; user-level units live at `~/.config/systemd/user/`; `systemctl --user enable scryd.service` + `loginctl enable-linger <user>` makes it survive logout / start at boot.

## §3 Decisions

1. **IPC channel.** Unix domain socket. Rationale: filesystem permissions (`0600`, owner = the operator) reject non-owner connections at the syscall level before any application code runs — strictly stronger than TCP-loopback + `SO_PEERCRED` and trivially proves the §2 acceptance criterion that non-owner connection attempts are rejected before any application-level command is dispatched.
2. **Socket path.** `$XDG_RUNTIME_DIR/scryd/scryd.sock`, with the parent dir created mode `0700`. Rationale: `$XDG_RUNTIME_DIR` is per-uid, mode `0700`, owned by the user, managed by systemd-logind — exactly the durability and ownership shape we need. Falls back to a daemon-startup error (clear message) if `$XDG_RUNTIME_DIR` is unset, since that means we have no safe place to put the socket.
3. **Defense-in-depth peer check.** On `accept(2)`, the daemon also calls `getsockopt(SO_PEERCRED)` and rejects the connection if the peer uid does not equal the daemon's own uid. Rationale: belt-and-braces against any future deployment shape (e.g., a packaging mistake that leaves the socket world-accessible). Cheap. Logs the rejection in the spec's `non-owner-user connection rejection` failure category.
4. **Config directory and file.** Config at `$XDG_CONFIG_HOME/scryd/config.toml`, mode `0600`, owner = the operator. Parent dir `$XDG_CONFIG_HOME/scryd/` mode `0700`. Rationale: standard XDG; the operator owns and writes their own config; nothing outside their uid can read the credential.
5. **Data directory.** `$XDG_DATA_HOME/scryd/`, mode `0700`, owner = the operator. Contains everything from the storage slice (`meta.sqlite`, `witchcraft.sqlite`, `raw/`, `bodies/`). Rationale: standard XDG; per-user; survives reboots.
6. **Service supervisor.** systemd user unit. Ship a unit file at `<install-share>/scryd.service` that the operator copies (or that an install helper places) into `~/.config/systemd/user/scryd.service`, then `systemctl --user enable --now scryd.service`. Rationale: spec already commits to systemd-class; user units are the per-user variant; no system-wide privilege required.
7. **Linger for "starts at boot".** Operator runs `loginctl enable-linger $USER` (one-time, requires admin help on a multi-user host). Rationale: this is the standard recipe to keep a user-level systemd manager alive across logouts and start it at boot; without it, the instance only runs while the user has an active login session. Documented in install steps; not enforced by the daemon.
8. **CLI auto-routing.** The CLI resolves the socket path the same way the daemon does: `$XDG_RUNTIME_DIR/scryd/scryd.sock`. No flag, no env var override, no config-file lookup. Rationale: an operator's CLI invocation can never be aimed at another operator's instance because every operator's `$XDG_RUNTIME_DIR` resolves to a different per-uid path. There is no override surface to misuse on a shared host. If `$XDG_RUNTIME_DIR` is unset, the CLI prints the same error the daemon would have printed and exits non-zero.
9. **Daemon startup self-checks.** Before opening the socket, the daemon verifies: (a) `$XDG_RUNTIME_DIR` exists and is owned by the running uid, mode `0700`; (b) `$XDG_CONFIG_HOME/scryd/config.toml` is mode `0600` and owned by the running uid; (c) `$XDG_DATA_HOME/scryd/` (creating it if missing) is mode `0700` and owned by the running uid. Any failure → refuse to start, log the specific permission/ownership problem, exit non-zero. Rationale: every cross-user-isolation promise depends on these invariants; verifying at startup is the cheapest place to fail loudly.
10. **No multi-instance-per-user.** A single user runs at most one scryd instance per host. Rationale: multi-instance-per-user would multiply socket-path conventions, supervisor-unit conventions, and confuse CLI auto-routing for no concrete need in v1. The instance is keyed by uid; if a user wants two mailbox sets, they configure two `[[accounts]]` entries in their one config file.
11. **CLI command transport.** The CLI talks to the local daemon over the same Unix-domain-socket HTTP API used by other callers — every CLI verb maps to an API operation (search → `/search`, add-account → an internal-only verb the api slice will detail, reindex → an internal-only verb routed to the search slice's `index_truncate` + queue-refill). Rationale: one transport, one access-control story, no second IPC mechanism. The `/reindex` endpoint is reachable only from the same uid (the spec's CLI-only constraint is satisfied by the same OS-level identity check that gates everything else); no application-level "is this the operator" check is needed because every connection is already that operator.

## §4 Contracts & shapes

Per-instance filesystem layout (under the operator's home-rooted XDG dirs):

- `$XDG_CONFIG_HOME/scryd/config.toml` — mode `0600`, owner = the operator. Contains accounts, IMAP credentials, sync settings.
- `$XDG_DATA_HOME/scryd/` — mode `0700`, owner = the operator. Inside: `meta.sqlite`, `witchcraft.sqlite`, `raw/`, `bodies/`. Layout details are owned by the storage slice.
- `$XDG_RUNTIME_DIR/scryd/` — mode `0700`, owner = the operator. Inside: `scryd.sock` (the IPC channel). The PID file, if any, lives here too — owned by the api or observability slice.
- `~/.config/systemd/user/scryd.service` — the user-level systemd unit. Owned by the build-and-packaging slice.

Daemon-startup invariants the daemon must enforce before opening the socket:

- `$XDG_RUNTIME_DIR` is set, exists, and is owned by the running uid with mode `0700`.
- `$XDG_CONFIG_HOME/scryd/config.toml` exists, is owned by the running uid, and is mode `0600`. (If absent on first start, the daemon refuses to start with an error pointing the operator at `scryd add-account`.)
- `$XDG_DATA_HOME/scryd/` exists (or is created by the daemon at first start), is owned by the running uid, and is mode `0700`.
- Each of the failures has a distinct error string mapped to the spec's `configuration permission error` failure category.

IPC channel contract:

- Transport: HTTP/1.1 over `AF_UNIX` stream socket at `$XDG_RUNTIME_DIR/scryd/scryd.sock`. The api slice owns the request/response shapes; this slice owns the socket lifecycle and the access-control gate.
- Permissions on the socket file: created mode `0600` (umask-cleared at bind), owner = the operator. The parent dir `$XDG_RUNTIME_DIR/scryd/` is mode `0700`.
- Accept-time check: the daemon calls `getsockopt(SO_PEERCRED)` on every accepted connection; if `peer.uid != self.uid`, the daemon writes a `non-owner-user connection rejection` log line including the peer uid and pid (when available) and closes the connection without reading any application bytes.

Failure logs this slice contributes (each maps to a closed-set category from the spec):

- `configuration permission error` — emitted by the startup self-checks (Decision 9). One log line per failed invariant; daemon exits non-zero.
- `non-owner-user connection rejection` — emitted by the accept-time `SO_PEERCRED` check (Decision 3) and by the kernel-level `EACCES` (the latter never reaches us because the kernel rejects before `accept(2)` returns; only the redundant accept-time check produces a log).

Stale-socket recovery:

- On startup, if `$XDG_RUNTIME_DIR/scryd/scryd.sock` already exists and a `connect(2)` to it fails, the daemon unlinks it and rebinds. Rationale: `$XDG_RUNTIME_DIR` is normally cleaned by systemd-logind across login sessions, but a hard-killed prior daemon can leave a stale socket within the same session. We do not unlink if `connect(2)` succeeds — that means another daemon is already running, in which case the new daemon exits non-zero with a "scryd is already running for this user" log line.

## §5 Sequence

1. **First-time install (operator).** Operator copies the systemd user unit into `~/.config/systemd/user/scryd.service` (or runs an install helper). Operator runs `scryd add-account` (cli slice) which writes `$XDG_CONFIG_HOME/scryd/config.toml` mode `0600`. Operator runs `systemctl --user enable --now scryd.service`. (For boot-survival: `loginctl enable-linger $USER`.)
2. **Daemon startup.** systemd --user spawns the daemon as the operator's uid → daemon runs the startup self-checks (Decision 9) → on success, daemon creates `$XDG_RUNTIME_DIR/scryd/`, binds `scryd.sock` mode `0600`, begins serving. On any self-check failure, daemon writes the appropriate `configuration permission error` log line and exits non-zero; systemd reports the unit failed.
3. **Caller connection (owning user).** Caller process running as the operator opens `$XDG_RUNTIME_DIR/scryd/scryd.sock` → kernel allows the connection (mode `0600`, owner match) → daemon `accept`s → daemon calls `getsockopt(SO_PEERCRED)` → uid matches → connection serves application requests.
4. **Caller connection (different non-owner user).** Caller process running as a different uid resolves what it thinks is scryd's socket. If it knows the path (`/run/user/<other-uid>/scryd/scryd.sock`), the kernel rejects `connect(2)` with `EACCES` because the parent dir `/run/user/<other-uid>/` is mode `0700` owner = the other user. The connection never reaches the daemon. Nothing is logged in the non-owner's instance because there is no instance to log in; the owning instance is unaffected.
5. **Caller connection from same host's hostile process running as same uid.** Excluded from the spec's threat model (any process running as the operator already has full file-read access to their data) but called out here for completeness: the OS check passes, the application requests succeed. This is by design.
6. **CLI invocation.** Operator types `scryd search <query>` → CLI binary resolves `$XDG_RUNTIME_DIR/scryd/scryd.sock` → connects → emits an HTTP `GET /search?…` over the socket → reads the response → renders. If `connect(2)` fails with `ENOENT`, the CLI prints "scryd is not running for user $USER. Start it with: systemctl --user start scryd" and exits non-zero. If `connect(2)` fails with `ECONNREFUSED` (socket exists, no listener), the CLI prints the same error.
7. **Reindex CLI verb.** Operator types `scryd reindex` → CLI dispatches an internal command over the same socket → daemon (api slice) routes to the search-engine slice's `index_truncate` + queue-refill flow → CLI streams or polls progress (api slice owns the stream shape).
8. **Daemon shutdown.** systemd sends SIGTERM → daemon stops accepting new connections, drains in-flight requests, unlinks `scryd.sock`, exits 0. systemd-logind cleans `$XDG_RUNTIME_DIR/scryd/` on session end.
9. **Two operators on one host.** alice and bob each run their own instance. alice's socket is at `/run/user/<alice-uid>/scryd/scryd.sock`; bob's at `/run/user/<bob-uid>/scryd/scryd.sock`. The parent dirs `/run/user/<alice-uid>/` and `/run/user/<bob-uid>/` are mode `0700` owned by their respective users. alice's CLI resolves to her socket via `$XDG_RUNTIME_DIR`; bob's CLI to his. Neither CLI can be flag-redirected at the other's socket because there is no flag.

## §6 Out of scope

- The HTTP request/response shapes carried over the socket (api slice).
- The `meta.sqlite` and `bodies/` and `raw/` schema/layout inside `$XDG_DATA_HOME/scryd/` (storage slice).
- The systemd user unit file's contents (build-and-packaging slice). This slice commits that one will exist at `~/.config/systemd/user/scryd.service`; build-and-packaging fills it in.
- `add-account` interactive credential capture (cli slice).
- Logging mechanism (observability slice). This slice declares which failure categories it emits; observability owns the log target.
- Defense against the superuser. The spec already places root outside the trust model.
- macOS / BSD / Windows. The spec is Linux-only.

## §7 Open questions

- Behavior when the operator's `$XDG_RUNTIME_DIR` is unset (e.g., a non-systemd Linux, or an SSH session with no logind PAM module loaded). The current decision is "refuse to start with a clear error". Confirm this is acceptable, or whether scryd should fall back to a path under `$HOME` (which would lose the kernel's `tmpfs` cleanup semantics).
- Should `scryd add-account` be invocable when the daemon is not running, writing the config directly, or does it require the daemon running and route the write through the API? The current sequence (§5 step 1) assumes "write directly to the config file"; the cli slice will pick the final answer.
- Whether the daemon should drop additional capabilities at startup (e.g., `seccomp` filters, `prctl(PR_SET_NO_NEW_PRIVS)`). These are belt-and-braces and overlap with the systemd unit's `NoNewPrivileges=`, `SystemCallFilter=` directives — defer to build-and-packaging.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
