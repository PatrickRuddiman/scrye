Parent slice: [scryd v0.2.0 — security](../../slices/0.2.0/security.md)
Depends on: none

# Task 01 — peercred-allowed-uid

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Make the daemon's peercred check accept a configured peer UID via `SCRYD_ALLOWED_UID` (cached once at startup), preserve `getuid()` fallback when the env var is unset, extend the `NON_OWNER_REJECTION` log line to carry `expected_uid`, and prove both paths in tests.

## Tasks
- [ ] In `crates/scryd-api/src/peercred.rs`, add a new helper `expected_peer_uid()` that reads the `SCRYD_ALLOWED_UID` environment variable, parses it as `u32`, and returns the parsed value. On a non-empty unparseable value the helper returns a typed error (new `ApiError::AllowedUidParse { raw: String }`). On unset/empty, the helper falls back to `nix::unistd::getuid().as_raw()`.
- [ ] Cache the resolved expected-UID at process startup using a `std::sync::OnceLock<u32>` initialised by a new `peercred::init()` function. The function runs once at daemon startup before the API server begins accepting connections; subsequent reads are lock-free.
- [ ] In `crates/scryd-api/src/peercred.rs:34` (`check_stream_peer`), replace the call to `current_uid()` (line 41) with a read from the `OnceLock`. Keep the existing `current_uid()` function in place as the fallback path used inside `expected_peer_uid`.
- [ ] In `crates/scryd-api/src/peercred.rs:24-28` (`check_peer_uid`'s `log_failure!` invocation), add an `expected_uid = expected_uid` field to the `NON_OWNER_REJECTION` record alongside the existing `peer_uid` and `peer_pid`.
- [ ] In `crates/scryd-api/src/lib.rs` (or wherever `ApiError` is defined), add `AllowedUidParse { raw: String }` variant. Daemon startup propagates this through the `RuntimeError` chain so systemd marks the unit failed with a clear cause.
- [ ] Wire `peercred::init()` into the daemon's serve path. Call site is `scryd-runtime` or wherever the API server starts (currently `crates/scryd-api/src/serve.rs`); call before the listener is opened. If the runtime serve path is still deferred (task 16 from v0.1.0), attach `peercred::init()` to the unit-test setup that exercises the API harness — the call must be reachable from any code path that opens a listener.
- [ ] Add `serial_test = "3"` to `crates/scryd-api/Cargo.toml` `[dev-dependencies]`.
- [ ] Add unit tests to `crates/scryd-api/tests/peercred.rs`: `env_unset_falls_back_to_getuid`, `env_set_to_valid_uid_returns_parsed_value`, `env_set_to_empty_string_falls_back`, `env_set_to_garbage_returns_parse_error`. Each test wraps `serial_test::serial` because env-var manipulation is process-global.
- [ ] Create new file `crates/scryd-api/tests/peercred_accept_path.rs`. The test binds a real `tokio::net::UnixListener` in a `tempfile::TempDir`, sets `SCRYD_ALLOWED_UID` to the test process's `getuid()`, calls `peercred::init()`, opens a `UnixStream` from the same process to that socket, runs `check_stream_peer` against the accepted half, asserts `Ok(uid)` with the right uid. A second sub-test sets `SCRYD_ALLOWED_UID` to `getuid() + 1`, runs the same setup, asserts `Err(ApiError::NonOwner)` and that exactly one `NON_OWNER_REJECTION` log record was emitted with `expected_uid` matching the env-var value.

## Acceptance criteria
- [ ] `cargo test -p scryd-api --test peercred` passes (all four env-var unit tests green).
- [ ] `cargo test -p scryd-api --test peercred_accept_path` passes (both real-UDS sub-tests green).
- [ ] `git grep -n 'SCRYD_ALLOWED_UID' crates/scryd-api/src/peercred.rs` matches the env-var read site.
- [ ] `git grep -nE 'expected_uid\s*=' crates/scryd-api/src/peercred.rs` matches the log-emission line.
- [ ] `git grep -n 'OnceLock' crates/scryd-api/src/peercred.rs` matches the cache.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
