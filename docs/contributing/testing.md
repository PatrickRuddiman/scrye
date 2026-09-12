---
sources:
  - .github/workflows/ci.yml
  - tests/
  - crates/scryd-runtime/tests/serve.rs
  - crates/scryd-imap/tests/greenmail_smoke.rs
  - ops/install.sh
---

# Testing

scryd is tested with Cargo's test harness plus a few cross-cutting shell and
Python harnesses. This page describes how the tests are organized and how to run
them.

## Layout

- **Unit tests** live alongside the code in each crate.
- **Integration tests** live in each crate's `tests/` directory.
- **Live integration tests** (files ending `_live.rs`, such as the IMAP backfill,
  incremental, IDLE, and scheduler tests) talk to a real IMAP server. CI provides
  one with a GreenMail service container.
- **Cross-cutting harnesses** live in the top-level `tests/`:
  - `install_sh.sh` — a smoke test of the system installer.
  - `e2e_imap_to_search.sh` — a full IMAP-to-search run that self-relaunches in a
    privileged container so it can install and start the daemon, then queries it.
  - `mcp_client.py` — a minimal MCP client the end-to-end test drives.

## Running the suite

```
cargo test --workspace --target <triple>
```

Run on a Unix target so the witchcraft engine is present. The witchcraft tests
need the model bundle staged in the assets directory, and the `_live` tests need a
reachable IMAP server. See [Weights and assets](../operations/weights-and-assets.md).

## Test-only hooks

A few seams exist purely for tests:

- `SCRYD_TEST_FAKE_FETCHER` and `SCRYD_FETCH_WEIGHTS_BIN` let a test substitute a
  fake weight-fetch helper so startup does not download anything.
- `install.sh` accepts `--skip-weights` and `--skip-systemctl` so the installer
  smoke test can run without fetching weights or talking to systemd.

## Continuous integration

The `ci` workflow runs on pushes and pull requests to `main`. It first runs a
license check (`cargo-deny`), then a test job on Ubuntu with a GreenMail service
that:

- verifies the systemd unit template with `systemd-analyze verify`,
- caches and downloads the model bundle,
- runs `cargo test --workspace`,
- builds the release binaries,
- runs the installer smoke test and the end-to-end IMAP-to-search test, and
- builds the native Linux packages.

See [Release and packaging](release-and-packaging.md) for the release workflow.

## See also

- [Development](development.md)
- [Repository layout](repo-layout.md)
- [Release and packaging](release-and-packaging.md)
- [MCP interface](../mcp/README.md)
