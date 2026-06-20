Parent slice: [mcp](../slices/mcp.md)
Depends on: 32, 33

# Task 34 — Retire scryd-api + docs/ops/CI

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
The `scryd-api` crate and its UDS/axum-0.7 footprint are removed, dead config keys are dropped, and config/ops/docs/CI describe the MCP surface — with the whole workspace building and testing green.

## Tasks
- [ ] Delete the `crates/scryd-api` directory and remove `"crates/scryd-api"` from the workspace `members` array at `Cargo.toml:10`.
- [ ] Remove the `axum = "0.7"` workspace dependency at `Cargo.toml:29` after confirming no remaining crate references the workspace `axum` (`git grep -nE "axum.*workspace|workspace.*axum" crates scryd`); remove the now-dead `require_peer_uid`/`socket_mode` fields, their `default_*` fns, and their use in `Default` from `crates/scryd-config/src/loader.rs:24-55`.
- [ ] Add a header note to `slices/api.md` marking it superseded by `slices/mcp.md`; update `README.md` and `ops/README.install.md` to document the MCP endpoint (`/mcp` on loopback), the mandatory `USER_EMAIL`, and the `SCRYD_MCP_BIND` env / `[server].mcp_bind` config.
- [ ] Update `ops/scryd.service.in` to add `Environment=USER_EMAIL=` (placeholder for the operator) and an optional `SCRYD_MCP_BIND`, and drop socket assumptions; update `ops/scryd.tmpfiles.in` (the `$XDG_RUNTIME_DIR/scryd` socket dir is no longer required) and remove socket references in `ops/install.sh` and `ops/uninstall.sh`.
- [ ] Update `.github/workflows/ci.yml` so the workspace build/test covers `scryd-mcp` (the `scryd-mcp` roundtrip test exercises the live MCP path) and remove any UDS-socket-specific assertion.

## Acceptance criteria
- [ ] `cargo build --workspace` exits 0.
- [ ] `cargo test --workspace` passes.
- [ ] `test ! -d crates/scryd-api`.
- [ ] `git grep -nE "scryd-api|scryd_api|require_peer_uid|socket_mode" -- crates scryd Cargo.toml` matches 0.
- [ ] `git grep -nE "USER_EMAIL" ops/scryd.service.in` matches at least 1.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
