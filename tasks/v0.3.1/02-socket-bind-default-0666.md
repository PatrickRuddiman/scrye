Parent plan: scryd v0.3.1 — service pivot
Depends on: 00

# Task 02 — socket-bind-default-0666

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Default the API socket to mode 0666 (open), drop the chgrp-to-dir-group dance entirely. The mode is now controlled by `[server] socket_mode` in config, defaulting to `0o666`. Operators who want the v0.2.0 group-restricted shape can flip the knob.

## Tasks
- [x] In `crates/scryd-api/src/socket.rs:14-43` (`bind`), add a `socket_mode: u32` parameter. The function signature becomes `pub async fn bind(scryd_runtime_dir: &Path, socket_mode: u32) -> Result<UnixListener, ApiError>`.
- [x] In `crates/scryd-api/src/socket.rs:46-50` (`inherit_dir_group` helper), delete the function. The socket no longer chowns to the directory's gid; the kernel-level network access control now collapses to "anyone who can reach the path".
- [x] In `crates/scryd-api/src/socket.rs:62-67` (`set_socket_mode_0660` helper), rename to `set_socket_mode(path, mode)` and take the mode as a parameter. Bind then calls `set_socket_mode(&socket_path, socket_mode)`.
- [x] In `crates/scryd-runtime/src/serve.rs` (the call site of `bind`), forward `cfg.server.socket_mode` from the loaded config.
- [x] Update `crates/scryd-api/tests/socket.rs::bind_creates_socket_at_mode_0660` (line 10): rename to `bind_honours_caller_supplied_mode`, parameterise on `0o666` (defaults), assert the resulting file mode equals what was passed.
- [x] Add `crates/scryd-api/tests/socket.rs::bind_creates_socket_at_mode_0660_when_requested` that re-asserts the v0.2.0 group-restricted behaviour by passing `0o660`.
- [x] Update the other tests in `crates/scryd-api/tests/socket.rs` (`stale_socket_left_by_a_dead_daemon_is_cleaned_up`, `bind_creates_parent_dir_at_mode_0700`, `second_bind_against_live_socket_returns_already_running`) to pass the new `socket_mode` argument through.

## Acceptance criteria
- [x] `cargo test -p scryd-api --test socket` passes (4 existing + 1 new).
- [x] `git grep -nE 'fn bind\(.*socket_mode' crates/scryd-api/src/socket.rs` matches.
- [x] `! git grep -F 'inherit_dir_group' crates/scryd-api/src/socket.rs` (the helper is gone).
- [x] `git grep -nE 'set_socket_mode\(' crates/scryd-api/src/socket.rs` matches the new generic name.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
