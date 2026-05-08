# scryd

**Turn your email into a searchable dataset AI agents can read.**

scryd is a per-user, read-only IMAP indexer + search daemon. It connects to
your mail account, mirrors the messages into a local SQLite store +
[witchcraft](https://github.com/dropbox/witchcraft)-backed semantic index, and
exposes a small HTTP API on a Unix-domain socket that local CLIs and AI agents
query against.

The point: an agent that needs context about a project, a person, a thread,
or a contract can pose a natural-language question and get back the
conversations that matter — with citations to the source messages — without
ever holding your mailbox credential.

## What it does

- **Indexes mail in the background.** scryd connects to IMAP using an app
  password, fetches messages, parses MIME, converts HTML to Markdown, and
  writes everything into `~/.local/share/scryd/`.
- **Serves search over a local socket.** Agents and CLIs talk to scryd over
  `$XDG_RUNTIME_DIR/scryd/scryd.sock`. The socket is mode `0700` and
  peercred-checked: only the daemon's own user can query it.
- **Three search modes.** `fulltext` (FTS5 keyword), `semantic` (T5 XTR
  embeddings via witchcraft), and `hybrid` (RRF fusion of the two).
- **Filters by sender, folder, account, and date range.** Same filter chain
  in every mode.
- **Returns full message bodies, threads, and raw `.eml` source** for the
  matches the agent needs to read.
- **Stores credentials only in `config.toml`** at mode `0600`. Agents that
  call scryd never see the IMAP password, only the search results.

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

Linux + macOS only in v0.1.0. Releases ship as per-arch tarballs on the
[Releases page](https://github.com/PatrickRuddiman/scrye/releases).

### Linux

```sh
tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
./install.sh
scryd add-account
systemctl --user enable --now scryd
```

`install.sh` is a per-user installer (refuses to run as root). It places the
binary in `~/.local/bin/`, the systemd user unit in
`~/.config/systemd/user/`, and runs `scryd-fetch-weights` to download the T5
weights into `$XDG_DATA_HOME/scryd/assets/`.

### macOS

```sh
tar -xzf scryd-vX.Y.Z-aarch64-macos.tar.gz
cd scryd-vX.Y.Z-aarch64-macos
mkdir -p ~/.local/bin && cp scryd scryd-fetch-weights ~/.local/bin/
~/.local/bin/scryd-fetch-weights
~/.local/bin/scryd add-account
~/.local/bin/scryd serve &
```

A launchd plist is a v0.2.0 follow-up; for now run `scryd serve` in the
foreground or wrap it in your shell of choice.

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
                                  │
                       ┌──────────┴──────────┐
                       ▼                     ▼
                meta.sqlite +           witchcraft
                raw .eml store      (T5 XTR embeddings)
```

One binary, one user-owned config, one user-owned data directory. No
system-wide privilege; multiple users on the same host run their own scryd
instances in isolation.

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
