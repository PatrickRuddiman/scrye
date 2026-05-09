Parent spec: [scryd v0.2.0](../../scryd-spec-v0.2.0.md)

# scryd v0.2.0 — security

## §1 Summary

Owns the runtime contract that turns "the daemon runs as a different Linux account than the operator" into the spec's hard property: an agent running under the operator's account cannot read the credential. Three concrete pieces: making the existing peercred check accept a configured peer UID via env var, wiring the unused `assert_mode_0600()` preflight at config-load time, and adding the test surface that proves both. The build-and-packaging slice owns the install-time placement of files; this slice owns the daemon's behavior at runtime once those files are in place.

## §2 Codebase reconnaissance

- Existing peercred check at `crates/scryd-api/src/peercred.rs:34-44`. `check_stream_peer` reads peer UID via `getsockopt(SO_PEERCRED)` (line 13), compares against `current_uid()` (line 41 — calls `getuid()`), logs `category::NON_OWNER_REJECTION` on mismatch, returns `ApiError::NonOwner`. Per-connection, called at accept time.
- Existing redaction at `crates/scryd-config/src/secret.rs:1-33`. `AccountPassword(SecretString)` — zeros on drop, custom `Debug` returns `[REDACTED]`. `Account.password` field is this type.
- Existing redaction tests at `crates/scryd-config/tests/loader.rs:179-184`. `account_password_debug_prints_redacted` asserts no leak of the literal in the Debug output.
- Existing storage assertion at `crates/scryd-storage/tests/reconcile.rs:148-163`. `no_password_column_in_accounts_table` proves the schema has no password column (PRAGMA table_info).
- Existing API assertion at `crates/scryd-api/tests/handlers_read.rs:224-235`. `/accounts` response test proves only `account_id` and `folders` appear in the JSON.
- Existing-but-unused permission check at `crates/scryd-config/src/permissions.rs:10-20`. `assert_mode_0600(path)` exported. `git grep -n assert_mode_0600` shows zero call sites in `src/` — the function never runs.
- Config load entrypoint at `crates/scryd-config/src/loader.rs:115-126`. `Config::load(path)` reads the TOML and returns the parsed struct; this is the natural call site for the preflight check.
- Logging category set at `crates/scryd-log/src/categories.rs`. `NON_OWNER_REJECTION` already defined; the slice adds no new category.
- Test harness for peercred at `crates/scryd-api/tests/peercred.rs`. Currently unit-tests `check_peer_uid` against synthetic uids; pattern extends naturally for the env-var.

## §3 Decisions

1. **Env-var name and shape.** Options: single `SCRYD_ALLOWED_UID` (one u32), `SCRYD_ALLOWED_UIDS` (CSV list), `SCRYD_ALLOWED_GROUP` (gid). **Chosen:** single `SCRYD_ALLOWED_UID`. Rationale: spec ships single-operator host; CSV / gid are speculative until the multi-operator trust mode is in scope.
2. **Fallback when env-var is unset.** Options: `getuid()` (preserves v0.1.0 behavior), refuse-to-start. **Chosen:** `getuid()` fallback. Rationale: the same binary supports both v0.1.0 per-user and v0.2.0 system-installed deployments without a feature flag; install.sh is the only thing that knows which mode it set up.
3. **When the env var is read.** Options: at every accept (fresh `std::env::var` per connection), once at startup (cache the resolved UID). **Chosen:** once at startup. Rationale: env vars don't change inside a long-running daemon process; per-accept reads burn syscalls.
4. **Wiring `assert_mode_0600()`.** Options: call at `Config::load`, leave for a later slice, make it warn-only. **Chosen:** call at `Config::load`, fail loud (return a typed error, daemon refuses to start). Rationale: the security property assumes mode 0600 holds; silently accepting mode 0644 violates the spec's "credential is absent from any file the operator's account can read" guarantee on a misconfigured host.
5. **Audit log on rejection.** Options: extend `NON_OWNER_REJECTION` to carry `expected_uid` alongside `peer_uid`, add a new category. **Chosen:** extend the existing record with `expected_uid` and `peer_pid` (peer_pid already there). Rationale: one category, more context per entry; matches the closed-set log discipline.
6. **Test surface.** Options: unit-only on the new `expected_peer_uid()` helper, integration-only with two real UDS clients, both. **Chosen:** both. Rationale: the unit test covers env-var parsing edge cases (unset, empty, malformed, valid); the integration test covers the actual accept-time reject path with the real `SO_PEERCRED` syscall in the loop.

## §4 Contracts & shapes

### `expected_peer_uid()` resolution

A helper added to `crates/scryd-api/src/peercred.rs` resolves the UID the daemon admits, in this order:

1. Read `SCRYD_ALLOWED_UID` from the process environment exactly once at startup, before the API server begins accepting connections. Cache the result.
2. If the env var is set and parses as a `u32`, use that value.
3. If the env var is set but does not parse as a `u32`, the daemon refuses to start with a typed error naming the env var and the unparseable input.
4. If the env var is unset or empty, fall back to `getuid()` (the v0.1.0 single-tenant behavior).

The cached value is plumbed into `check_stream_peer` (replacing the per-call `current_uid()` lookup) so every accepted connection uses the same expected UID for the lifetime of the process.

### Peer-rejection log entry

When the peercred check rejects a connection (peer UID does not match the expected UID), the daemon emits one log record under category `NON_OWNER_REJECTION` carrying:
- `peer_uid` — the UID the kernel reported for the connecting peer
- `peer_pid` — the PID the kernel reported (already present)
- `expected_uid` — the cached expected UID (newly added)

No other fields. The peer's program name, command line, or fd are NOT included — the daemon doesn't have them and resolving them risks racing process-exit.

### Config-load preflight

`Config::load(path)` calls `assert_mode_0600(path)` before the TOML deserialize step. On Unix, the check `stat()`s the path and verifies `mode & 0o777 == 0o600`. On a mismatch, the function returns `ConfigError::PermissionInvariant { path, mode_seen }` and `Config::load` propagates the error. The daemon's startup self-check refuses to proceed; systemd marks the service failed; the operator sees the failure in `journalctl -u scryd`.

### Test additions

- `crates/scryd-api/tests/peercred.rs` grows four cases for the helper:
  - env-unset → returns `getuid()`
  - env=`12345` → returns `12345`
  - env=`""` (empty) → returns `getuid()`
  - env=`"not-a-number"` → returns a typed error the daemon-startup path surfaces
  Each case uses `serial_test::serial` because env vars are process-global.
- A new integration test at `crates/scryd-api/tests/peercred_accept_path.rs` binds a real UDS, sets `SCRYD_ALLOWED_UID` to the test process's `getuid()`, accepts a connection from the same process, asserts `Ok`. Then sets `SCRYD_ALLOWED_UID` to `getuid() + 1` (a UID this process can't be) and asserts the connection is rejected with `ApiError::NonOwner` and one `NON_OWNER_REJECTION` log record was emitted with `expected_uid` set.
- `crates/scryd-config/tests/loader.rs` grows `load_refuses_world_readable_config`: write a config at mode 0644, call `Config::load`, assert `Err(ConfigError::PermissionInvariant)` with the right path and mode in the error. And `load_accepts_mode_0600_config`: same setup but mode 0600, assert `Ok`.
- The existing `account_password_debug_prints_redacted` and `no_password_column_in_accounts_table` tests stay; this slice adds no work for them.

## §5 Sequence

1. **Daemon starts.** systemd invokes `/usr/local/bin/scryd serve` with `Environment=SCRYD_ALLOWED_UID=__UID__` already substituted by install-time templating.
2. **Process startup resolves the expected UID once.** Before the API server accepts the first connection, the runtime calls the new helper, which reads `SCRYD_ALLOWED_UID` from the env, parses it, and caches it. Unparseable env aborts startup with a typed error.
3. **Process startup loads config.** `Config::load` calls `assert_mode_0600` on the config path. Mismatch → typed error → systemd marks the unit failed → operator notices.
4. **Operator's CLI dials the socket.** `scryd search ...` opens `/run/scryd/scryd.sock`. The kernel completes the connection.
5. **Daemon accept-side runs the peercred check.** `check_stream_peer` reads peer UID via `SO_PEERCRED`, compares against the cached expected UID. Match → handler runs, response sent. Mismatch → daemon emits one `NON_OWNER_REJECTION` log record with `peer_uid`, `peer_pid`, `expected_uid`, returns `ApiError::NonOwner`, closes the connection.
6. **Operator's adversary process attempts to read `/etc/scryd/config.toml`.** OS kernel returns EACCES because the file is `scryd:scryd 0600` and the adversary's UID is the operator's, not `scryd`. No daemon code involvement.
7. **Operator's adversary process attempts to ptrace the daemon.** YAMA blocks the cross-UID ptrace at the kernel layer. No daemon code involvement.
8. **Different Linux user `mallory` attempts to dial the socket.** mallory is not in the operator's group, so `/run/scryd/` (mode 0750 group=operator) is not list-readable to her — the connection attempt fails before reaching the daemon. If mallory is somehow in the group (operator misconfiguration), the daemon's peercred check rejects her UID with the same `NON_OWNER_REJECTION` log line that the operator's adversary would trigger.

## §6 Out of scope

- The `SCRYD_ALLOWED_UID` env-var injection itself — owned by the build-and-packaging slice's install-time unit templating.
- `/run/scryd/` and socket FS perms — owned by build-and-packaging.
- The CLI's elevation split (which verbs require sudo, what they print on no-elevation) — owned by the cli slice.
- Multiple expected UIDs / GID-based access — out of scope per spec single-operator host trust mode.
- OAuth, OS keychain, HSM — spec out-of-scope.
- A "least surprise" mode where unparseable `SCRYD_ALLOWED_UID` falls back to `getuid()` instead of aborting startup. The slice chose abort over silent fallback because the env-var-set case implies the operator wanted that specific UID; honoring `getuid()` in that case would silently weaken the property.

## §7 Open questions

- Whether the `peercred_accept_path.rs` integration test runs in the docker-based CI smoke (alongside `tests/install_sh.sh`) or stays a `cargo test` invocation only. Default position: keep it in `cargo test` (fast, runs on every push) and let the install smoke focus on the install layout.
- Whether `Config::load` should also assert the parent directory's mode (e.g., `/etc/scryd/` is mode 0700) or only the file. Default position: file-only for v0.2.0 — install.sh sets the dir mode and the spec's promise is about the credential file specifically.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
