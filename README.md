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

### Requirements

- Linux x86_64 or aarch64 with systemd (Ubuntu 22.04+, Debian 12+,
  Fedora 36+, Arch). The release tarballs are linked against
  glibc 2.34+; older distros need to build from source.
- Root (the installer creates a system user and writes under `/etc`,
  `/var/lib`, `/usr/local/bin`, and `/etc/systemd/system`).
- Internet access on first install for `scryd-fetch-weights` to
  download the T5 weights bundle (~1 GB; SHA-256-verified).

### Quick install

Download the per-arch tarball from the
[Releases page](https://github.com/PatrickRuddiman/scrye/releases),
extract, and run the installer:

```sh
tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
sudo ./install.sh
```

By default the installer resolves the operator from `$SUDO_USER`. To
install for a different login (e.g. provisioning a service account):

```sh
sudo ./install.sh --user alice
```

What `install.sh` lays down:

| Path | Owner | Mode | Purpose |
|---|---|---|---|
| `/usr/local/bin/scryd` | root:root | 0755 | CLI + daemon binary |
| `/usr/local/bin/scryd-fetch-weights` | root:root | 0755 | weights downloader |
| `/etc/scryd/config.toml` | scryd:scryd | 0600 | IMAP accounts (operator can't read) |
| `/var/lib/scryd/` | scryd:scryd | 0700 | meta DB + index |
| `/var/lib/scryd/assets/` | scryd:scryd | 0755 | T5 weights (mmap) |
| `/run/scryd/` | scryd:&lt;operator&gt; | 0750 | runtime dir; tmpfiles.d |
| `/etc/systemd/system/scryd.service` | root:root | 0644 | rendered unit |
| `/etc/tmpfiles.d/scryd.conf` | root:root | 0644 | rendered drop-in |

### First account

```sh
sudo scryd add-account
sudo systemctl restart scryd
```

The CLI prompts for IMAP host / port / user / password; the password
is written into `/etc/scryd/config.toml` (which only the daemon's UID
can read) and the daemon picks up the new account on the next start.
If the daemon was not yet running, the CLI prints
`apply changes: sudo systemctl start scryd` instead.

### Verify it's working

```sh
# Live tail the daemon log.
journalctl -u scryd -f

# Run a search (no sudo needed; the operator's UID is allowed
# on the socket).
scryd search "from:bob"

# Check the index size.
sudo ls /var/lib/scryd/
```

### Daily commands

```sh
# Reads (no sudo)
scryd search "lunch with bob since:2026-01-01"
scryd reindex

# Mutations (sudo because only scryd:scryd can write the config;
# each prints a restart hint)
sudo scryd add-account
sudo scryd rotate-password <account-id>
sudo scryd remove-account <account-id>
sudo systemctl restart scryd
```

### Uninstall

```sh
sudo ./uninstall.sh
```

Removes the system service, unit file, tmpfiles drop-in, FHS
directories, binaries, and the `scryd` Linux account. Idempotent.

For more — isolation property table, journalctl recipes, full
walkthrough — see [ops/README.install.md](ops/README.install.md).

## Isolation

The daemon runs as a dedicated `scryd` system user, which is what makes
the credential isolation possible: `/etc/scryd/config.toml` is mode
0600 owned by `scryd:scryd`, so any process running as the operator's
UID — interactive shells, agents, MCP servers — gets `EACCES` when it
tries to open the file. The operator still reaches the daemon over
`/run/scryd/scryd.sock` (group-readable to the operator), which is how
search queries arrive without exposing the IMAP password. See
[ops/README.install.md](ops/README.install.md#isolation-properties)
for the full property table.

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
operator-allowed socket. Single-operator per host.

## Local development

Build from source on Linux:

```sh
git clone https://github.com/PatrickRuddiman/scrye.git
cd scrye
cargo build --release -p scryd -p scryd-fetch-weights
```

The integration test suite hits a real IMAP server. Spin up
[GreenMail](https://greenmail-mail-test.github.io/greenmail/) in
Docker, then run `cargo test`:

```sh
docker run -d --rm --name greenmail \
  -p 3025:3025 -p 3143:3143 \
  -e GREENMAIL_OPTS="-Dgreenmail.smtp.hostname=0.0.0.0 -Dgreenmail.smtp.port=3025 \
                     -Dgreenmail.imap.hostname=0.0.0.0 -Dgreenmail.imap.port=3143 \
                     -Dgreenmail.users=test:test@localhost -Dgreenmail.auth.disabled" \
  greenmail/standalone:latest

cargo test --workspace
```

Tests that require GreenMail skip silently with a stderr hint when the
container isn't reachable. The full end-to-end smoke (install + index
+ search) runs as:

```sh
cargo build --release -p scryd -p scryd-fetch-weights
bash tests/e2e_imap_to_search.sh
```

## CI

Two GitHub Actions workflows:

- **`ci.yml`** — runs on every push to `main` and every pull request:
  cargo-deny check, `cargo test --workspace`, install.sh smoke test,
  end-to-end imap-to-search smoke (50 messages injected via SMTP →
  scryd indexes → `scryd search` returns hits). GreenMail runs as a
  GH Actions service container.
- **`release.yml`** — fires on `v*` tags and `workflow_dispatch`.
  Same test gates plus per-arch tarball packaging (x86_64 +
  aarch64), SHA-256 sums, and `gh release create`.

## License

[Apache-2.0](LICENSE).
