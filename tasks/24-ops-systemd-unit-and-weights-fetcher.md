Parent slice: [build-and-packaging](../slices/build-and-packaging.md)
Depends on: 16

# Task 24 — ops-systemd-unit-and-weights-fetcher

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Ship the systemd user unit (with the spec's hardening directives) and the `scryd-fetch-weights` helper binary that downloads/verifies the T5 GGUF weights into `$XDG_DATA_HOME/scryd/assets/`.

## Tasks
- [x] Create `ops/scryd.service` containing exactly the unit from build-and-packaging slice §3 Decision 8: `[Unit]` with `Description`, `After=network-online.target`, `Wants=network-online.target`; `[Service]` with `Type=simple`, `ExecStart=%h/.local/bin/scryd serve`, `Restart=on-failure`, `RestartSec=5`, hardening directives (`NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=read-only`, `ReadWritePaths=%h/.local/share/scryd %h/.config/scryd %t/scryd`, `PrivateTmp=yes`, `PrivateDevices=yes`, `LockPersonality=yes`, `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`, `SystemCallFilter=@system-service`, `SystemCallArchitectures=native`); `[Install]` with `WantedBy=default.target`. **Do not** include `MemoryDenyWriteExecute=yes` (slice §3 Decision 8 + §7).
- [x] Create a new binary crate `crates/scryd-fetch-weights/` with its own `Cargo.toml`. Add to the workspace members list in the root `Cargo.toml`. Deps: `reqwest` (with `rustls-tls`, `stream`), `tokio`, `sha2`, `hex`, `clap` (derive), `anyhow`. Add `[[bin]] name = "scryd-fetch-weights"`.
- [x] In `crates/scryd-fetch-weights/src/main.rs`, define a clap arg `--target <dir>` defaulting to `$XDG_DATA_HOME/scryd/assets/` (fall back to `$HOME/.local/share/scryd/assets/`). Refuse to run as root (`if uid == 0 → exit 1`).
- [x] Define `const WEIGHTS_URL: &str = "<huggingface url>"` and `const WEIGHTS_SHA256: &str = "<hex>"` baked into source — set both at task-execution time by reading the upstream witchcraft README's pinned weight reference and computing the SHA-256.
- [x] Implement: if `<target>/xtr-weights.gguf` exists and matches `WEIGHTS_SHA256`, exit 0 with stdout `weights ok`. Else create `<target>/` mode `0700`; download to `<target>/xtr-weights.gguf.tmp` mode `0600` via reqwest+rustls; stream with progress to stderr; verify SHA-256; on mismatch delete the tmp and exit 1; rename `tmp` → `xtr-weights.gguf`; exit 0 with stdout `weights ok (<size> bytes)`.
- [x] Write integration tests in `crates/scryd-fetch-weights/tests/fetch.rs`: spin up a hyper test server on `127.0.0.1` that serves a small known-bytes payload with a known SHA-256; pass that URL via a `--mock-url` test-only override (gated behind `#[cfg(test)]` or a `--mock-url` clap arg hidden in release). Assert: missing file triggers download → file present mode `0600` with matching hash; second invocation is a no-op; corrupt file is rejected and re-downloaded.
- [x] Document in `ops/README.install.md` (created in Task 25) the `WEIGHTS_URL` and SHA-256 so operators with no internet can side-load the file.
- [x] Write a shell-based smoke test in `tests/scryd_service_unit.sh` (a tiny script) that runs `systemd-analyze verify ops/scryd.service` if available, else `grep` the file for the required directives. Wire it into a `cargo make` task or a `Makefile` target named `verify-unit`.

## Acceptance criteria
- [x] `cargo build --release -p scryd-fetch-weights` exits 0; produces `target/release/scryd-fetch-weights`.
- [x] `cargo test -p scryd-fetch-weights` passes.
- [x] `test -f ops/scryd.service`.
- [x] `grep -E '^NoNewPrivileges=yes' ops/scryd.service` matches.
- [x] `grep -E '^ProtectHome=read-only' ops/scryd.service` matches.
- [x] `grep -E '^ReadWritePaths=' ops/scryd.service | grep -F '%t/scryd'` matches.
- [x] `grep -cE '^MemoryDenyWriteExecute' ops/scryd.service` outputs `0`.
- [x] `grep -E '^ExecStart=%h/\.local/bin/scryd serve' ops/scryd.service` matches.
- [x] `git grep -nE 'WEIGHTS_SHA256\s*:\s*&str\s*=' crates/scryd-fetch-weights/src/main.rs` matches a non-empty literal.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
