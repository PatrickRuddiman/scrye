Parent plan: scryd v0.3.1 — service pivot
Depends on: 00

# Task 01 — peercred-opt-in

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Make the accept-time peercred check off by default. Existing call paths only enforce it when `[server] require_peer_uid = true`. The init / cache / log shape stays so a deploy that flips the knob gets the v0.2.0 behaviour back.

## Tasks
- [x] In `crates/scryd-api/src/peercred.rs:72` (`check_stream_peer`), add a leading `enabled: bool` parameter; when false, return `Ok(extract_peer_uid(stream)?)` without a comparison or log emission. The helper still returns the peer uid for telemetry, just doesn't reject.
- [x] In `crates/scryd-api/src/serve.rs:23` (where `init_peercred()?` runs unconditionally), gate the call on the resolved `require_peer_uid`. The serve signature gains a `require_peer_uid: bool` parameter (or accepts the whole `ServerCfg` and reads the field). When the knob is off, skip `init_peercred()` and pass `enabled = false` into every `check_stream_peer` call inside the accept loop.
- [x] In `crates/scryd-runtime/src/serve.rs` (where the runtime constructs the api router and calls `scryd_api::serve(...)`), pull `cfg.server.require_peer_uid` out of the loaded `Config` and forward it.
- [x] Update existing tests in `crates/scryd-api/tests/peercred.rs` to pass `enabled = true` explicitly to `check_stream_peer` so the assertion shape is preserved.
- [x] Add a new test `crates/scryd-api/tests/peercred.rs::check_stream_peer_returns_ok_when_disabled` that calls `check_stream_peer(&stream, /*enabled=*/false)` from a UDS pair where the peer uid does NOT match the expected uid; asserts `Ok(...)` and that no `non-owner-user connection rejection` log line was emitted.
- [x] Refresh `crates/scryd-api/tests/peercred_accept_path.rs` to set `enabled = true` for the existing accept/reject sub-tests; add one sub-test where `enabled = false` and a mismatched peer uid still goes through.

## Acceptance criteria
- [x] `cargo test -p scryd-api --test peercred --test peercred_accept_path` passes (8 + 3 = 11 tests).
- [x] `git grep -nE 'fn check_stream_peer' crates/scryd-api/src/peercred.rs` shows the new `enabled: bool` parameter.
- [x] `git grep -nE 'check_stream_peer\(' crates/scryd-api/src/serve.rs` matches and the call passes a boolean derived from config.
- [x] `git grep -nE 'init_peercred\(\)' crates/scryd-api/src/serve.rs` is wrapped in an `if`-style guard, not a bare statement.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
