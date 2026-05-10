# scryd

**An IMAP indexer + search service. Install once per server, link any number of accounts, query an open API tagged by `account_id`.**

scryd is a Linux service. Operators install it on a server, configure
`[[accounts]]` in `/etc/scryd/config.toml`, and let it index continuously
in the background via IMAP IDLE + a [witchcraft](https://github.com/dropbox/witchcraft)-backed
semantic index. The HTTP-over-UDS API is open by default — anyone who can
reach `/run/scryd/scryd.sock` can query the full index. The consumer's
higher-layer API service is the auth boundary: it authenticates end-users,
decides which `account_ids` each is allowed to see, and passes that filter
on every search call.

## What it does

- **Indexes mail in the background.** scryd connects to IMAP using an app
  password (per account), fetches messages, parses MIME, converts HTML to
  Markdown, and writes everything into `/var/lib/scryd/` + a persistent
  witchcraft index at `/var/lib/scryd/witchcraft.sqlite`.
- **Serves search over a local socket.** `/run/scryd/scryd.sock` mode 0666
  by default; configurable via `[server] socket_mode`.
- **Multi-account by design.** Documents carry their `account_id`; the
  search API takes a multi-value `?account_ids=a,b,c` filter so the
  consumer can scope each query.
- **Three search modes.** `fulltext` (BM25), `semantic` (T5 XTR embeddings
  via witchcraft), and `hybrid` (RRF fusion of the two).
- **Filters by sender, folder, account, and date range.** Same filter chain
  in every mode.
- **Returns full message bodies, threads, and raw `.eml` source.**
- **No auth at the scryd layer.** Auth is the consumer's job — see
  [`docs/security.md`](docs/security.md) for the threat model.

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

## Security posture

scryd is open by default: anyone who can reach `/run/scryd/scryd.sock`
can call the API. The dedicated `scryd` system user owns the config
file and the index, so non-root non-`scryd`-group users still can't
read the IMAP credential at rest — but the search API has no auth.

**The consumer's API is the auth boundary.** It authenticates
end-users, decides which `account_ids` each is allowed to see, and
passes that filter on every search call (`?account_ids=a,b,c`).
Empty filter = all accounts.

See [`docs/security.md`](docs/security.md) for the full threat model
and the consumer's checklist. Hardening knobs (`[server]
require_peer_uid = true`, `socket_mode = 0o660`) live in
[`scryd-spec.md`](scryd-spec.md).

## Use

### From the CLI

```sh
scryd search "annual invoice from acme"
scryd search "birthday plans" --since 2026-01-01 --mode semantic --limit 10
scryd search "contract terms" --accounts work,personal --folder INBOX --json
scryd sync                              # signal every supervisor
scryd status                            # daemon uptime + per-account state
```

### From an agent

The agent shells out to `scryd search ... --json` (the simplest contract) or
talks to the UDS directly:

```
GET /search?q=annual%20invoice&mode=hybrid&limit=10
GET /search?q=invoice&account_ids=alice-personal,support-inbox
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
   IMAP server(s) ──IDLE/poll──▶  scryd  ──UDS──▶  consumer's API ──▶ end-user
                                    │              (open socket; consumer
                       ┌────────────┴────────┐      filters by account_ids)
                       ▼                     ▼
                meta.sqlite +           witchcraft.sqlite
                raw .eml store          (persistent T5 XTR
                  (scryd:scryd)          embeddings)
```

One scryd install per server. N IMAP accounts. Each indexed
document carries its `account_id`. Consumer's higher-layer API is
the auth boundary.

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
