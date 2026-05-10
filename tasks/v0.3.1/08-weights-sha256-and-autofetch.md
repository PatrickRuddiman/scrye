Parent plan: scryd v0.3.1 — service pivot
Depends on: 06, 07

# Task 08 — weights-sha256-and-autofetch

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Bake the real SHA-256 of the upstream `xtr-weights.gguf` into `scryd-fetch-weights` (replacing the all-zero placeholder), and have the daemon shell out to the fetcher on first start when the weights file is missing. Operators no longer need `--sha256` overrides; the daemon self-heals on a fresh install.

## Tasks
- [x] Compute the SHA-256 of `https://huggingface.co/dropbox/witchcraft-weights/resolve/main/xtr-weights.gguf` once on a fresh download. Replace the placeholder constant `DEFAULT_WEIGHTS_SHA256` at `crates/scryd-fetch-weights/src/main.rs:24-25` with the real hex value (lowercase, 64 chars).
- [x] Update the doc-comment immediately above (`crates/scryd-fetch-weights/src/main.rs:18-23`) to describe the baked-in hash + the override mechanism for air-gapped installs.
- [x] In `crates/scryd-runtime/src/serve.rs:serve_init` (just before the `WitchcraftIndexer::open` call from task 06), check whether `<assets_dir>/xtr-weights.gguf` exists. When missing:
  - Spawn `scryd-fetch-weights --target <assets_dir>` via `std::process::Command::new("scryd-fetch-weights")`.
  - Resolve the binary via `which`-style PATH lookup; fall back to `/usr/local/bin/scryd-fetch-weights` (the install.sh-installed location) if PATH lookup fails.
  - Wait for completion; on non-zero exit, fail `serve_init` with a clear `RuntimeError::PermissionInvariant { path: assets_dir.join("xtr-weights.gguf"), reason: "weights auto-fetch failed: ..." }`.
  - Emit a `log_lifecycle!(kind = kind::STARTUP, info = "fetching xtr-weights.gguf on first start")` line so the journalctl trail explains the long pause.
- [x] In `tests/install_sh.sh`, the existing `--skip-weights` flag stays so the smoke harness doesn't pull 1 GB. Document in the harness comment header that the flag is test-only; production never sets it.
- [x] Add a runtime test `crates/scryd-runtime/tests/serve.rs::serve_init_invokes_fetcher_when_weights_missing` (gated on a test-only env var `SCRYD_TEST_FAKE_FETCHER=path/to/fake.sh`): the harness writes a tiny shell script that creates `<target>/xtr-weights.gguf` (zero-byte stub) and exits 0; sets `SCRYD_FETCH_WEIGHTS_BIN=<that-path>`; runs `serve_init`; asserts the file was created. The runtime reads `SCRYD_FETCH_WEIGHTS_BIN` env var as an override before falling back to PATH/install lookup.
- [x] Add the same env-var override (`SCRYD_FETCH_WEIGHTS_BIN`) to `crates/scryd-runtime/src/serve.rs` so the test can substitute a fake fetcher.

## Acceptance criteria
- [x] `cargo test -p scryd-runtime --test serve serve_init_invokes_fetcher_when_weights_missing` passes (or skips silently when the env var isn't set).
- [x] `! git grep -F '0000000000000000000000000000000000000000000000000000000000000000' crates/scryd-fetch-weights/src/main.rs` (the placeholder is gone).
- [x] `git grep -nE 'DEFAULT_WEIGHTS_SHA256' crates/scryd-fetch-weights/src/main.rs` matches and the constant is a 64-char lowercase hex string.
- [x] `git grep -nE 'scryd-fetch-weights' crates/scryd-runtime/src/serve.rs` matches the auto-fetch invocation.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
