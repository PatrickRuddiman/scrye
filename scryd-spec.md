# scryd — IMAP Indexer & MCP Search Daemon

## §1 Summary

scryd is a single-purpose daemon. One process serves one mailbox: it fetches the IMAP account whose login matches the mandatory `USER_EMAIL` environment variable, indexes every message with the witchcraft semantic engine, and serves search over a Model Context Protocol (MCP) server bound to loopback TCP (`127.0.0.1:7878` by default). There is no CLI client, no HTTP API, and no other control surface — the MCP server is the only way in.

Top-level promises:

- The IMAP credential at rest is owned by the dedicated `scryd` system user and the file's mode (0640) keeps non-`scryd`-group users from reading it.
- The IMAP server is treated strictly as read-only — scryd never sends, deletes, or flags messages.
- `USER_EMAIL` is the authorization boundary. The daemon fetches, indexes, and returns **only** the account(s) whose IMAP login equals `USER_EMAIL` (case-insensitive). It refuses to start when `USER_EMAIL` is unset or empty.
- The MCP listener binds a loopback address only; it refuses any routable interface.
- A new mail message arriving on the IMAP server becomes visible to a search query within seconds (via IMAP IDLE) or one polling interval (when IDLE isn't supported).

## §2 Personas

- **Operator** — the human or automation who installs scryd on a host, sets `USER_EMAIL` on the unit, configures the matching `[[accounts]]` entry in the config TOML, and runs `systemctl start scryd`. Owns the box.
- **MCP client** — a local AI assistant / agent framework (Claude Desktop, a custom agent, your own code) that connects to `http://127.0.0.1:7878/mcp` and calls the read-only tools. It is trusted because it runs on the same host as the daemon and the daemon serves a single mailbox owner.

## §3 Scope

### In

- One scryd daemon per mailbox. The daemon keeps only the configured `[[accounts]]` whose `user` equals `USER_EMAIL`; any other account in the file is invisible.
- Background indexing via IMAP IDLE (when supported) and polling fallback.
- WitchcraftIndexer-backed semantic + full-text + hybrid search, persisted to `witchcraft.sqlite`.
- An MCP server over Streamable HTTP on loopback TCP, advertising six read-only tools: `search`, `get_message`, `get_raw_message`, `get_thread`, `list_accounts`, `status`.
- Per-call re-resolution of the owned `account_id`(s); every tool filters its result to them, and an out-of-scope id returns `not_found` (no existence leak).
- Per-account custom-CA TLS path for self-signed corporate IMAP servers (`tls_ca_path` field on `[[accounts]]`).
- A privileged-docker e2e harness that proves the inject-via-SMTP → IMAP fetch → witchcraft index → MCP-search round-trip in CI.

### Out

- **A CLI client / HTTP API / Unix-socket surface.** Removed. Fetch and index run automatically inside the daemon; search is served only over MCP. There are no `sync`/`reindex`/`add-account` controls — configuration is the on-disk TOML, picked up at start.
- **Bearer-token / OAuth auth on the MCP endpoint.** v1's boundary is loopback-only + single-mailbox `USER_EMAIL` scoping. Token auth is a future follow-up; the architecture admits it cleanly.
- **Multi-user tenancy.** One `USER_EMAIL` per daemon process.
- **Indexing of attachment contents.** Subject, sender, body text, and attachment file names are indexed; PDF/docx/zip contents are not parsed.
- **Sending mail.** scryd is read-only against IMAP.
- **OAuth / token-based IMAP auth.** App passwords only.
- **Cross-OS support.** Linux only. macOS / Windows ports are out (the witchcraft backend and several daemon crates are Linux-only).

## §4 Quality bars

- p95 search-end-to-end latency under 200 ms for a corpus of 10 000 messages on a warm cache.
- p99 search latency under 500 ms, same conditions.
- The daemon achieves at least 99.5% successful response rate over rolling 24-hour windows on a host with continuous IMAP connectivity.
- New mail visible to a search query within 60 seconds when IDLE is available, within the configured `[sync] poll_interval_seconds` otherwise.
- The IMAP credential never appears in any process other than the daemon's process memory; the file permits read access only to the `scryd` system user (and members of the `scryd` group).
- The release runs on x86_64 and aarch64 Linux against current Debian, current Ubuntu LTS, current Fedora, and current Arch.

### Trust model

scryd ships one trust mode: **single-mailbox loopback daemon**. The MCP server binds `127.0.0.1` (never a routable interface) and `USER_EMAIL` scopes every response to the one mailbox the daemon owns. Any local process that can reach the loopback port can call the read-only tools — the operator chooses the host boundary (a personal workstation, or a server where only trusted agents run). There is no per-caller authentication in v1.

A future trust mode, **token-authenticated MCP**, in which the server checks a per-caller bearer token, is out of scope for v1. The architecture admits it cleanly via a future `[server] auth = "bearer"` configuration.

## §5 Failure modes

The daemon emits structured JSON-Lines logs through `journalctl -u scryd`. Categories the operator should expect:

- `missing USER_EMAIL` — the mandatory env var is unset/empty; the daemon exits non-zero before binding anything.
- `connect failure` — TCP connect to IMAP server failed (network, DNS, port closed).
- `tls failure` — TLS handshake failed (cert chain not trusted, no shared cipher).
- `auth rejection` — IMAP server rejected the credential.
- `push channel drop` — IDLE socket dropped mid-session; the supervisor reconnects with backoff.
- `single-message parse failure` — one message's MIME parsing produced a `ParsedDegraded` or `Unparseable` outcome; the storage row + raw `.eml` still land.
- `configuration permission error` — preflight check on the config file mode failed.
- `crash loop backoff` — repeated unclean shutdowns within a short window trigger capped exponential backoff at startup (issue #20); the MCP `status` tool reports `ok=false` while a loop is active.

## §6 Out-of-scope security claims

- scryd does NOT defend against an attacker who has root on the host. Root reads the config file, the daemon's process memory, and the witchcraft sqlite verbatim.
- scryd does NOT defend against another local process on the same host reaching the loopback MCP port. Host-level access control is the operator's responsibility; the v1 boundary is loopback + single-mailbox scoping, not per-caller auth.
- scryd does NOT defend against a misconfigured `USER_EMAIL`. Whatever address it is set to defines the served mailbox; setting it to the wrong account serves the wrong account.

See [`docs/security.md`](docs/security.md) for the full threat model.
