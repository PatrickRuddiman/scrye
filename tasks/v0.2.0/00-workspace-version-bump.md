Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md)
Depends on: none

# Task 00 — workspace-version-bump

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Bump the workspace version from `0.1.0` to `0.2.0` so every member crate inherits the new release line and `cargo metadata` reports the v0.2.0 development cycle.

## Tasks
- [ ] In `Cargo.toml`, change `[workspace.package].version = "0.1.0"` to `version = "0.2.0"`.
- [ ] Refresh `Cargo.lock` by running `cargo metadata --format-version=1 --offline >/dev/null 2>&1 || cargo fetch` (offline-friendly; the lock-file's version entries auto-update on the next cargo invocation).

## Acceptance criteria
- [ ] `git grep -nE '^version = "0\.2\.0"' Cargo.toml` matches the workspace.package line.
- [ ] `cargo metadata --format-version=1 --no-deps 2>/dev/null | python -c 'import sys,json; assert all(p["version"] == "0.2.0" for p in json.load(sys.stdin)["packages"]), "version drift"'` exits 0.
- [ ] `cargo check --workspace` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
