---
sources:
  - Cargo.toml
  - scryd/src/main.rs
  - crates/
  - ops/
  - scripts/
  - tests/
  - .github/
---

# Repository layout

scryd is a Cargo workspace. One thin binary crate wires together nine library
crates, each owning a single slice of the daemon's job.

## Workspace crates

| Crate | Responsibility |
| --- | --- |
| `scryd` | The daemon binary. A thin entry point that caps worker threads, installs a TLS provider, and calls `scryd_runtime::serve()`. |
| `scryd-runtime` | Process lifecycle: startup ordering, preflight, path resolution, crash-loop backoff, and wiring the other crates together. See [Daemon lifecycle](../concepts/lifecycle.md). |
| `scryd-config` | The TOML config schema, loading, validation, file-permission checks, and the redacting secret type. See [Configuration](../configuration.md). |
| `scryd-imap` | IMAP connection, backfill, IDLE/poll loops, the per-account supervisor tree, and the read-only verb allow-list. See [IMAP sync](../concepts/imap-sync.md). |
| `scryd-mime` | MIME parsing into a Markdown body and typed headers, defensive caps, and encryption handling. See [MIME and Markdown](../concepts/mime-and-markdown.md). |
| `scryd-storage` | The `meta.sqlite` schema and migrations, raw `.eml` files, threading, the index queue, and the daemon-run journal. See [Storage](../concepts/storage.md). |
| `scryd-search` | The witchcraft and in-memory index engines, search modes, the index drainer, and snippet derivation. See [Indexing and search](../concepts/indexing-and-search.md). |
| `scryd-mcp` | The MCP server, the read-only tools, DTOs, mailbox scoping, and the error model. See [MCP interface](../mcp/README.md). |
| `scryd-log` | Structured logging and the closed-set log categories and kinds. See [Observability](../concepts/observability.md). |
| `scryd-fetch-weights` | A standalone helper binary that downloads, verifies, and extracts the witchcraft model bundle. See [Weights and assets](../operations/weights-and-assets.md). |

## Top-level directories

| Path | Contents |
| --- | --- |
| `crates/` | The nine library crates above. |
| `scryd/` | The daemon binary crate. |
| `ops/` | The installer and uninstaller, the systemd unit template, the nfpm config and maintainer scripts, and the install/release READMEs. |
| `scripts/` | `build-packages.sh`, which renders the unit and runs nfpm. |
| `tests/` | Cross-cutting harnesses: the installer smoke test, the IMAP-to-search end-to-end test, and a minimal MCP client. |
| `.github/` | CI, release, and asset workflows plus the package-building action. |
| `docs/` | This documentation tree. |
| `slices/` | Design slices. Some predate the shipped code; where they disagree, the code wins. |
| `tasks/` | Per-task breakdowns. |

## See also

- [Architecture](../concepts/architecture.md)
- [Development](development.md)
- [Testing](testing.md)
- [Release and packaging](release-and-packaging.md)
