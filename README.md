# scryd

**A single-purpose mail daemon: it fetches one IMAP mailbox, indexes it with
[witchcraft](https://github.com/dropbox/witchcraft), and serves semantic search
over a local [MCP](https://modelcontextprotocol.io) server — and nothing else.**

scryd does exactly three things:

1. **Fetches** the IMAP mailbox whose login equals the mandatory `USER_EMAIL`
   environment variable (IMAP IDLE + polling, in the background).
2. **Indexes** every message with a witchcraft-backed T5 XTR semantic index.
3. **Serves search** over a Model Context Protocol server bound to loopback TCP
   (`127.0.0.1:7878` by default), so any local MCP-capable AI client can query
   the mailbox.

There is **no CLI client and no other control surface**. The MCP server is the
only way in, and every response is scoped to `USER_EMAIL` — the daemon serves a
single mailbox owner and refuses to start unscoped.

## Why an agent should use it

Email is most people's largest unindexed knowledge source — receipts, invoices,
contracts, decisions, introductions, the conversation history behind every
project. scryd makes that history queryable in the shape an agent already speaks:
MCP tools that return short ranked snippets with stable IDs you can expand to the
full message, thread, or raw `.eml`.

## The MCP surface

scryd speaks MCP over Streamable HTTP at `http://127.0.0.1:7878/mcp` (override
with `SCRYD_MCP_BIND`; the address must stay on a loopback interface). It
advertises six read-only tools:

| Tool | Purpose |
|---|---|
| `search` | Ranked hits across the mailbox (fulltext / semantic / hybrid), with sender / folder / date filters. |
| `get_message` | One parsed message by id (body as Markdown + headers). |
| `get_raw_message` | The raw RFC 5322 `.eml` source of a message. |
| `get_thread` | Every message in a thread, oldest first. |
| `list_accounts` | The configured account(s) owned by `USER_EMAIL`. |
| `status` | Daemon health: uptime, per-account sync state, drainer + crash-loop status. |

Fetching and indexing run automatically inside the daemon — there are no
`sync`/`reindex`/`add-account` tools and no write surface of any kind.

### Scoping (`USER_EMAIL`)

`USER_EMAIL` is mandatory and is the daemon's only authorization boundary:

- At startup scryd keeps only the configured `[[accounts]]` whose `user` (IMAP
  login) equals `USER_EMAIL` (case-insensitive), so it fetches and indexes
  **only that mailbox**.
- Every MCP tool re-resolves the owned `account_id`(s) on each call and filters
  results to them. A message id belonging to any other account returns
  `not_found` — the same answer a truly missing id gets, so existence never
  leaks.
- If `USER_EMAIL` is unset or empty the daemon exits non-zero before binding
  anything.

## Components

| Crate / binary | Purpose |
|---|---|
| `scryd` | The daemon binary (no subcommands; runs fetch + index + MCP serve) |
| `scryd-mcp` | The MCP server: scoped tools over Streamable HTTP |
| `scryd-fetch-weights` | Helper that downloads + verifies the T5 GGUF weights on first start |
| `scryd-storage` | SQLite metadata + raw-eml store + write queue |
| `scryd-mime` | MIME parse + HTML→Markdown |
| `scryd-imap` | IMAP client + scheduler |
| `scryd-search` | Witchcraft semantic indexer |
| `scryd-runtime` | Daemon orchestration + preflight |
| `scryd-config` / `scryd-log` | Config types + JSON-line logging |

## Install

### Requirements

- Linux x86_64 or aarch64 with systemd (Ubuntu 22.04+, Debian 12+, Fedora 36+,
  Arch). Release tarballs link against glibc 2.34+; older distros build from
  source. (scryd does **not** build on Windows or macOS — the witchcraft backend
  and several daemon crates are Linux-only.)
- Root, to create the system user and write under `/etc`, `/var/lib`,
  `/usr/local/bin`, and `/etc/systemd/system`.
- Internet on first start for `scryd-fetch-weights` to download the witchcraft
  asset bundle (~61 MB, SHA-256-verified): `config.json` + `tokenizer.json` +
  `xtr.gguf`.

### Quick install

```sh
tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
sudo ./install.sh
```

`install.sh` is non-interactive and idempotent. Flags `--skip-weights` and
`--skip-systemctl` exist for test environments.

What it lays down:

| Path | Owner | Mode | Purpose |
|---|---|---|---|
| `/usr/local/bin/scryd` | root:root | 0755 | daemon binary |
| `/usr/local/bin/scryd-fetch-weights` | root:root | 0755 | asset-bundle downloader |
| `/etc/scryd/config.toml` | scryd:scryd | 0640 | IMAP account (group `scryd` can read; world cannot) |
| `/var/lib/scryd/` | scryd:scryd | 0700 | meta DB + witchcraft index |
| `/var/lib/scryd/assets/` | scryd:scryd | 0755 | xtr asset bundle |
| `/etc/systemd/system/scryd.service` | root:root | 0644 | the unit |

### Configure the mailbox

scryd serves the mailbox named by `USER_EMAIL`, so you must set it **and** add a
matching `[[accounts]]` entry whose `user` equals that address.

1. Set `USER_EMAIL` on the unit (the daemon won't start until you do):

   ```sh
   sudo systemctl edit scryd
   # add under [Service]:
   #   Environment=USER_EMAIL=alice@example.com
   ```

2. Hand-edit `/etc/scryd/config.toml` (owned by `scryd:scryd`, mode 0640):

   ```toml
   [[accounts]]
   id = "primary"
   host = "imap.example.com"
   port = 993
   user = "alice@example.com"   # must equal USER_EMAIL
   password = "..."
   tls = true
   folders = ["INBOX"]
   # optional per-account custom CA for self-signed corporate IMAP:
   # tls_ca_path = "/etc/scryd/work-ca.pem"

   # optional: override the loopback MCP bind (default 127.0.0.1:7878)
   # [server]
   # mcp_bind = "127.0.0.1:7878"
   ```

3. Start it:

   ```sh
   sudo systemctl restart scryd
   journalctl -u scryd -f
   ```

### Connect an MCP client

Point any MCP client (Claude Desktop, an agent framework, your own code) at the
Streamable-HTTP endpoint:

```
http://127.0.0.1:7878/mcp
```

Then call the tools — e.g. `search` with `{"q": "annual invoice from acme",
"mode": "hybrid", "limit": 10}`, and `get_message` / `get_thread` /
`get_raw_message` to expand a hit. Use `status` to confirm the daemon is healthy
and the account is indexing (`accounts[].last_seen_uid` flips to non-null once
the first message is fetched).

### Uninstall

```sh
sudo ./uninstall.sh
```

Removes the service, unit file, FHS directories, binaries, and the `scryd` Linux
account. Idempotent.

For the full walkthrough — isolation property table, journalctl recipes, asset
bundle details — see [ops/README.install.md](ops/README.install.md).

## Security posture

scryd binds MCP to a **loopback** TCP address only and refuses any routable
interface. Within the host, the boundary is `USER_EMAIL`: the daemon only ever
fetches and returns the one mailbox it is scoped to. The dedicated `scryd` system
user owns the config and index (mode 0640 / 0700), so other non-root users can't
read the IMAP credential at rest.

There is no bearer-token / OAuth auth on the MCP endpoint yet — local-loopback +
single-mailbox scoping is the v1 boundary; token auth is a future follow-up. See
[`docs/security.md`](docs/security.md) for the threat model.

## Architecture in one breath

```
   IMAP server ──IDLE/poll──▶  scryd  ──MCP/loopback HTTP──▶  AI client
   (USER_EMAIL's mailbox)        │        (127.0.0.1:7878/mcp,
                    ┌────────────┴────────┐   scoped to USER_EMAIL)
                    ▼                     ▼
             meta.sqlite +           witchcraft.sqlite
             raw .eml store          (persistent T5 XTR
               (scryd:scryd)          embeddings)
```

One daemon, one mailbox, one local search surface.

## Local development

scryd's daemon crates are Linux-only (witchcraft + the Unix runtime). Build and
test on Linux:

```sh
git clone https://github.com/PatrickRuddiman/scrye.git
cd scrye
cargo build --release -p scryd -p scryd-fetch-weights
cargo test --workspace
```

On a Windows or macOS host, build and test inside a Linux container (e.g.
`rust:1-bookworm` with `build-essential cmake clang libclang-dev pkg-config
libssl-dev git`) — the witchcraft backend won't compile natively.

Integration tests that hit a real IMAP server use
[GreenMail](https://greenmail-mail-test.github.io/greenmail/); they skip silently
when it isn't reachable. The full end-to-end smoke (real fetch → index → MCP
search round-trip) is:

```sh
docker run -d --rm --name greenmail \
  -p 3025:3025 -p 3143:3143 \
  -e GREENMAIL_OPTS="-Dgreenmail.smtp.hostname=0.0.0.0 -Dgreenmail.smtp.port=3025 \
                     -Dgreenmail.imap.hostname=0.0.0.0 -Dgreenmail.imap.port=3143 \
                     -Dgreenmail.users=test:test@localhost -Dgreenmail.auth.disabled" \
  greenmail/standalone:latest

cargo build --release -p scryd -p scryd-fetch-weights
bash tests/e2e_imap_to_search.sh
```

## CI

- **`ci.yml`** — every push to `main` and every PR: cargo-deny check,
  `cargo test --workspace`, systemd-unit verification, install.sh smoke test, and
  the end-to-end fetch → index → MCP-search smoke against a GreenMail service
  container.
- **`release.yml`** — fires on `v*` tags and `workflow_dispatch`: the same gates
  plus per-arch tarball packaging (x86_64 + aarch64), SHA-256 sums, and
  `gh release create`.

## License

[Apache-2.0](LICENSE).
