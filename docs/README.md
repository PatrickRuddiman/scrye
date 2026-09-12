---
sources:
  - README.md
  - scryd-spec.md
---

# scryd documentation

scryd is a single-purpose daemon. It connects to one IMAP mailbox, keeps a local
copy and search index of that mailbox, and answers search and message-retrieval
requests over a Model Context Protocol (MCP) server bound to loopback. It is the
search backend an MCP-aware assistant queries to read your mail; it is not a mail
client, a web service, or a writable store.

## What scryd does

- Fetches exactly one account — the IMAP login that matches the mandatory
  `USER_EMAIL` environment variable — using read-only IMAP verbs.
- Parses each message, stores metadata in SQLite and the raw RFC 822 source on
  disk, and indexes the text with the witchcraft engine (full-text plus a T5 XTR
  semantic index).
- Serves six read-only MCP tools (`search`, `get_message`, `get_raw_message`,
  `get_thread`, `list_accounts`, `status`) over Streamable HTTP on
  `127.0.0.1:7878` by default.

## What scryd is not

- It has no command-line client and no subcommands; the `scryd` binary only
  accepts `--help` and `--version`. All search goes through MCP.
- It never mutates the mailbox. The IMAP layer permits only non-destructive
  verbs.
- It does not listen on a routable address. The MCP bind address must be
  loopback.

## Documentation map

- [Getting started](./getting-started.md) — zero to a running daemon answering a
  search.
- [Installation](./installation.md) — native packages, the tarball installer,
  file layout, upgrades, and removal.
- [Configuration](./configuration.md) — the full `config.toml` schema,
  environment variables, and path resolution.
- [MCP interface](./mcp/README.md) — transport, scoping, the error model, and one
  page per tool.
- Concepts — how the daemon works:
  - [Architecture](./concepts/architecture.md)
  - [Daemon lifecycle](./concepts/lifecycle.md)
  - [IMAP sync](./concepts/imap-sync.md)
  - [Indexing and search](./concepts/indexing-and-search.md)
  - [Storage](./concepts/storage.md)
  - [MIME and Markdown](./concepts/mime-and-markdown.md)
  - [Observability](./concepts/observability.md)
- Operations:
  - [Running the daemon](./operations/running.md)
  - [Upgrades](./operations/upgrades.md)
  - [Weights and assets](./operations/weights-and-assets.md)
  - [Troubleshooting](./operations/troubleshooting.md)
- Contributing:
  - [Repository layout](./contributing/repo-layout.md)
  - [Development](./contributing/development.md)
  - [Testing](./contributing/testing.md)
  - [Release and packaging](./contributing/release-and-packaging.md)
- [Security model](./security.md) — trust boundaries, secret handling, and the
  read-only guarantee.
