Parent slice: [scryd v0.2.0 — cli](../../slices/0.2.0/cli.md)
Depends on: none

# Task 08 — cli-sudo-path-resolution

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Extend the CLI's config-path resolution and UDS-socket-path resolution so a sudo'd CLI invocation finds `/etc/scryd/config.toml` and `/run/scryd/scryd.sock` even though sudo strips the daemon's `XDG_*` environment, while keeping v0.1.0 per-user paths working when running unprivileged.

## Tasks
- [ ] In `scryd/src/main.rs:361-377` (`resolve_config_path`), insert a new branch between the `XDG_CONFIG_HOME` check and the `HOME` fallback: if `nix::unistd::geteuid().as_raw() == 0` and the previous `XDG_CONFIG_HOME` lookup produced no path, return `PathBuf::from("/etc/scryd/config.toml")`. The unprivileged-with-`HOME` path stays last (v0.1.0 read-only verbs continue to work without root).
- [ ] In `scryd/src/uds_client.rs` (`UdsClient::from_env`), add a fallback chain: try `$XDG_RUNTIME_DIR/scryd/scryd.sock` first; if that path does not exist, try `/run/scryd/scryd.sock`; if neither exists, return `ClientError::DaemonNotRunning`. The order matters — v0.1.0 deployments still resolve the per-user socket from `XDG_RUNTIME_DIR`.
- [ ] Add `nix = { version = "0.29", default-features = false, features = ["user"] }` to `scryd/Cargo.toml` `[dependencies]` if not already inherited transitively. Verify by running `cargo tree -p scryd --depth 1 | grep -F nix`.
- [ ] Create `scryd/tests/path_resolution.rs` with four tests: `config_path_uses_xdg_when_set` (set `XDG_CONFIG_HOME`, assert path), `config_path_uses_etc_scryd_when_root_and_xdg_unset` (mock root via env-var injection or a small testable helper that takes a `uid: u32` arg; cleanest is a `resolve_config_path_for(uid, env)` function that the entrypoint wraps), `config_path_uses_home_when_unprivileged` (HOME set, expect `~/.config/scryd/config.toml`), `socket_path_falls_back_to_run_scryd_when_xdg_runtime_dir_socket_missing` (TempDir-driven; create the `/run/scryd/scryd.sock` equivalent in a tempdir and use a path-injection helper).
- [ ] Refactor `resolve_config_path()` and `UdsClient::from_env()` to take a `Resolver` argument or split out testable helpers (e.g., `resolve_config_path_for(euid: u32, env: &dyn EnvSource)`). Keep the public function signatures the same; the helpers are `pub(crate)` for testing.

## Acceptance criteria
- [ ] `cargo test -p scryd --test path_resolution` passes (all four tests).
- [ ] `git grep -F '/etc/scryd/config.toml' scryd/src/main.rs` matches the EUID==0 fallback.
- [ ] `git grep -F '/run/scryd/scryd.sock' scryd/src/uds_client.rs` matches the runtime-dir fallback.
- [ ] `cargo build -p scryd` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
