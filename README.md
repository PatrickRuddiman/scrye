# scryd

**Turn your email into a searchable dataset AI agents can read.**

scryd is a read-only IMAP indexer + search daemon for Linux. It connects
to your mail account, mirrors the messages into a local SQLite store +
[witchcraft](https://github.com/dropbox/witchcraft)-backed semantic index,
and exposes a small HTTP API on a Unix-domain socket that local CLIs and
AI agents query against.

The point: an agent that needs context about a project, a person, a thread,
or a contract can pose a natural-language question and get back the
conversations that matter — with citations to the source messages — without
ever holding your mailbox credential.

## What it does

- **Indexes mail in the background.** scryd connects to IMAP using an app
  password, fetches messages, parses MIME, converts HTML to Markdown, and
  writes everything into `/var/lib/scryd/`.
- **Serves search over a local socket.** Agents and CLIs talk to scryd over
  `/run/scryd/scryd.sock`. The socket is peercred-checked: only the
  operator's UID can connect.
- **Three search modes.** `fulltext` (FTS5 keyword), `semantic` (T5 XTR
  embeddings via witchcraft), and `hybrid` (RRF fusion of the two).
- **Filters by sender, folder, account, and date range.** Same filter chain
  in every mode.
- **Returns full message bodies, threads, and raw `.eml` source** for the
  matches the agent needs to read.
- **Keeps the IMAP credential out of the operator's reach.** The daemon
  runs under a dedicated `scryd` system user; `/etc/scryd/config.toml` is
  mode 0600 owned by that user. The operator's shell, agents, and MCP
  servers can search but cannot read the password.

## Why an agent should use it

Email is most organizations' largest unindexed knowledge source — receipts,
invoices, contracts, agreements, decisions, introductions, the conversation
history behind every project. scryd makes that history queryable in the same
shape an agent already uses for code search and document retrieval: short
ranked snippets with stable IDs you can fetch the full context for.

## Components

| Crate / binary | Purpose |
|---|---|
| `scryd` | The CLI + daemon (`scryd add-account`, `scryd search`, `scryd reindex`, `scryd serve`) |
| `scryd-fetch-weights` | Helper that downloads + verifies the T5 GGUF weights on first install |
| `scryd-storage` | SQLite metadata + raw-eml store + write queue |
| `scryd-mime` | MIME parse + HTML→Markdown |
| `scryd-imap` | IMAP client + scheduler |
| `scryd-search` | Witchcraft binding (`WitchcraftIndexer` / `InMemoryIndexer`) |
| `scryd-api` | Axum router over UDS |
| `scryd-runtime` | Daemon orchestration + preflight |
| `scryd-config` / `scryd-log` | Config types + JSON-line logging |

## Install

scryd v0.2.0 ships Linux x86_64 / aarch64 tarballs on the
[Releases page](https://github.com/PatrickRuddiman/scrye/releases).

```sh
tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
sudo ./install.sh
```

The installer creates a `scryd` system user, lays out
`/etc/scryd`, `/var/lib/scryd`, `/run/scryd`, renders the systemd unit,
fetches the T5 weights, and starts the daemon. See
[ops/README.install.md](ops/README.install.md) for the full walkthrough
(sudo install, isolation properties, daily-use commands, journalctl
recipes, uninstall, v0.1.0 → v0.2.0 migration).

## Isolation

The daemon runs as a dedicated `scryd` system user, which is what makes
the credential isolation possible: `/etc/scryd/config.toml` is mode
0600 owned by `scryd:scryd`, so any process running as the operator's
UID — interactive shells, agents, MCP servers — gets `EACCES` when it
tries to open the file. The operator still reaches the daemon over
`/run/scryd/scryd.sock` (group-readable to the operator), which is how
search queries arrive without exposing the IMAP password. See
[ops/README.install.md](ops/README.install.md#isolation-properties)
for the full table and [scryd-spec-v0.2.0.md](scryd-spec-v0.2.0.md)
for the threat model.

## Use

### From the CLI

```sh
scryd search "annual invoice from acme"
scryd search "birthday plans" --since 2026-01-01 --mode semantic --limit 10
scryd search "contract terms" --account work --folder INBOX --json
```

### From an agent

The agent shells out to `scryd search ... --json` (the simplest contract) or
talks to the UDS directly:

```
GET /search?q=annual%20invoice&mode=hybrid&limit=10
```

Returns ranked hits with `message_id`, `score`, snippet, sender, subject,
date, folder. Then:

```
GET /message/<id>
GET /thread/<id>
GET /message/<id>/raw
```

…to pull the full body, the surrounding thread, or the raw `.eml` source.

## Architecture in one breath

```
   IMAP server  ──IDLE/poll──▶  scryd  ──UDS──▶  CLI / agent
                                  │              (operator UID)
                       ┌──────────┴──────────┐
                       ▼                     ▼
                meta.sqlite +           witchcraft
                raw .eml store      (T5 XTR embeddings)
                  (scryd:scryd UID)
```

One binary, one system-wide config under `scryd:scryd`, one
operator-allowed socket. v0.2.0 is single-operator per host; multi-
operator support is deferred to v0.3.0.

## Status

scryd is published as a series of testable, committed building blocks:
storage, MIME parse, witchcraft binding, API handlers, CLI verbs, install
script, CI release matrix. Most of the pieces are unit/integration-tested;
the live IMAP connection + daemon orchestration are in progress (see
`tasks/` for the per-task status). Until those land, `scryd serve` exits
cleanly with a deferred-integration message, and the CLI verbs talk to the
UDS but the daemon isn't running yet.

## License

[Apache-2.0](LICENSE).
