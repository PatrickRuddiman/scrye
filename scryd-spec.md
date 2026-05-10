# scryd — IMAP Indexer & Search Service

## §1 Summary

scryd is a service. One install per server indexes any number of IMAP accounts; documents in the index carry a stable `account_id`. The search API is open by default — anyone who can reach `/run/scryd/scryd.sock` can query the full index. The consumer's higher-layer API (whatever exposes scryd to end-users) is the auth boundary: it authenticates end-users, decides which `account_ids` each is allowed to see, and passes that filter on every search call.

Top-level promises:

- The IMAP credential at rest is owned by the dedicated `scryd` system user and the file's mode (0640) keeps non-`scryd`-group users from reading it. The daemon is not part of any per-end-user trust boundary.
- The IMAP server is treated strictly as read-only — scryd never sends, deletes, or flags messages.
- The search API is open. Filtering by `account_ids` is the consumer's responsibility, not scryd's.
- A new mail message arriving on the IMAP server becomes visible to a search query within seconds (via IMAP IDLE) or one polling interval (when IDLE isn't supported).

## §2 Personas

- **Operator** — the human or automation who installs scryd on a server, configures `[[accounts]]` in `/etc/scryd/config.toml`, and runs `sudo systemctl start scryd`. Owns the box.
- **Consumer** — the higher-layer API service that wraps scryd. Calls `scryd search`, `GET /search`, `GET /message/:id`, `GET /thread/:id`, `GET /accounts`, `GET /status`, `POST /sync`. Holds the responsibility for authentication, RBAC, account-scope enforcement, and rate-limiting.
- **End-user** — the human or agent on the other side of the consumer's API. Never talks to scryd directly.

## §3 Scope

### In

- One scryd daemon per server. Multiple `[[accounts]]` per daemon, each with a stable `account_id`.
- Background indexing of every configured account via IMAP IDLE (when supported) and polling fallback.
- WitchcraftIndexer-backed semantic + full-text + hybrid search, persisted to `/var/lib/scryd/witchcraft.sqlite`.
- Open HTTP-over-UDS API at `/run/scryd/scryd.sock` (mode `0666` by default; configurable).
- Caller-driven `account_ids` query filter on `GET /search` (and analogous endpoints). Empty filter = all accounts.
- Per-account custom-CA TLS path for self-signed corporate IMAP servers (`tls_ca_path` field on `[[accounts]]`).
- A CLI (`scryd add-account`, `scryd rotate-password`, `scryd remove-account`, `scryd search`, `scryd reindex`, `scryd sync`, `scryd status`, `scryd serve`).
- A privileged-docker e2e harness that proves the inject-via-SMTP → index → search round-trip in CI.

### Out

- **Auth at scryd's API layer.** The consumer's higher-layer API is the boundary. scryd ships open by default; an opt-in `[server] require_peer_uid = true` is the only knob. OAuth, JWT, mTLS, RBAC, and per-end-user account-scope enforcement are all the consumer's responsibility.
- **Indexing of attachment contents.** Subject lines, sender, body text, and attachment file names are indexed; PDF/docx/zip contents are not parsed.
- **Sending mail.** scryd is read-only against IMAP.
- **GUI / web UI.** scryd ships a CLI + the local API; the consumer builds whatever surface end-users see.
- **OAuth / token-based IMAP auth.** App passwords only. v0.3.5 will add OAuth.
- **Cross-OS support.** Linux only. macOS / Windows ports are out.
- **`uninstall.sh --dry-run`.** The uninstall script does its work directly; the artifact list is documented.

## §4 Quality bars

- p95 search-end-to-end latency under 200 ms for a corpus of 10 000 messages on a warm cache.
- p99 search latency under 500 ms, same conditions.
- The daemon achieves at least 99.5% successful response rate over rolling 24-hour windows on a host with continuous IMAP connectivity.
- New mail visible to a search query within 60 seconds when IDLE is available, within the configured `[sync] poll_interval_seconds` otherwise.
- The IMAP credential never appears in any process other than the daemon's process memory; the file permits read access only to the `scryd` system user (and members of the `scryd` group, which the operator can grant by adding admins to it).
- The release runs on x86_64 and aarch64 Linux against current Debian, current Ubuntu LTS, current Fedora, and current Arch.

### Trust model

scryd ships one trust mode: **open service**. The operator places scryd behind whatever auth boundary they want — typically a private network plus an upstream API service. scryd does not authenticate callers; it does not know which end-user is on the other side of the consumer's API. `account_ids` filtering on the search query is the only mechanism for scoping a result set; it is enforced honestly (the daemon respects the filter), but the consumer must populate it correctly.

A future trust mode, **token-authenticated service**, in which the daemon checks a per-caller bearer token, is not in v0.3.x. The architecture admits it cleanly via a future `[server] auth = "bearer"` configuration; v0.3.x's spec does not constrain that future shape.

## §5 Failure modes

The daemon emits structured JSON-Lines logs through `journalctl -u scryd`. Categories the operator should expect:

- `connect failure` — TCP connect to IMAP server failed (network, DNS, port closed).
- `tls failure` — TLS handshake failed (cert chain not trusted, no shared cipher).
- `auth rejection` — IMAP server rejected the credential.
- `push channel drop` — IDLE socket dropped mid-session; the supervisor reconnects with backoff.
- `non-owner-user connection rejection` — only when `[server] require_peer_uid = true` and a peer with a non-matching uid connects. Default off.
- `single-message parse failure` — one message's MIME parsing produced a `ParsedDegraded` or `Unparseable` outcome; the storage row + raw `.eml` still land.
- `configuration permission error` — preflight check on `/etc/scryd/config.toml` mode failed.

## §6 Out-of-scope security claims

- scryd does NOT defend against an attacker who has root on the host. Root reads `/etc/scryd/config.toml`, the daemon's process memory, and the witchcraft sqlite verbatim.
- scryd does NOT defend against an attacker who has reached the API socket without going through the consumer's API. Network-level access control (firewalls, private networks, mTLS terminators in front of the consumer's API) is the operator's responsibility.
- scryd does NOT defend against the consumer's API neglecting to pass an `account_ids` filter. The default empty filter returns all accounts; that is by design.

See [`docs/security.md`](docs/security.md) for the full threat model + a checklist for the consumer.
