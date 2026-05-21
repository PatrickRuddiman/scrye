# scryd security posture

scryd is a service (v0.3.x service shape). It ships **open by default** so it can be a clean cog in any consumer's indexing flow without reinventing OAuth, RBAC, or per-end-user account scope. This document spells out exactly what scryd defends against and what it doesn't, so the consumer can fill the gap.

## What scryd defends against

- **IMAP credential at rest is not world-readable.** `/etc/scryd/config.toml` is mode `0640` owned by `scryd:scryd`. A non-root user who is not in the `scryd` group cannot read it.
- **Read-only against IMAP.** scryd's command set is structurally restricted to the read-only IMAP verb allow-list (`EXAMINE`, `UID FETCH`, `UID SEARCH`, `IDLE`, `NOOP`, `LOGOUT`, `CAPABILITY`). It cannot send, delete, flag, or move messages on the IMAP server even if compromised.
- **Indexed content is not executable.** scryd parses MIME, extracts text + Markdown body, indexes them. It does not render HTML, execute scripts, or open attachments.
- **TLS to the IMAP server is real.** Default `webpki-roots` trust anchors; per-account `tls_ca_path` for self-signed corporate CAs. There is no skip-verify option.
- **Logged stderr is structured + scrubbed.** Credentials never enter the log line; the `AccountPassword` type's `Debug` impl prints `[REDACTED]`.

## What scryd does NOT defend against

- **End-user authentication.** scryd does not know who is on the other side of the consumer's API. It serves any caller who can reach the socket.
- **Per-end-user account-scope enforcement.** The `account_ids` query filter is honoured honestly — but if the consumer's API forgets to set it, scryd returns hits across every account in the index.
- **Network-level access to the socket.** Anyone who can reach `/run/scryd/scryd.sock` (or whatever address `[server] socket_mode` allows) can call the API. The operator chooses the firewall / network boundary.
- **Root on the host.** Root reads the config file, the witchcraft sqlite, and the daemon's process memory verbatim.
- **Side-channels in the search response.** Hit counts, snippets, and `account_id` are visible to any caller. Don't deploy scryd in a context where the existence of an account is itself a secret.

## Your job as the consumer

1. **Authenticate end-users at your API.** Don't expose scryd's socket directly to untrusted networks.
2. **Decide which `account_ids` each end-user is allowed to see** based on whatever policy you enforce (group membership, ownership, customer tenancy, etc.).
3. **Pass that filter on every search call.** Empty / missing = all accounts. The endpoint is `GET /search?account_ids=alice-personal,support-inbox`. Same shape applies to forwarded `--accounts` on the CLI.
4. **Rate-limit at your API.** scryd has no built-in rate limiter; a chatty caller can saturate the indexer.
5. **Audit at your API.** scryd logs the request shape (method, path, status, duration) but not the calling identity — because there isn't one.

## Hardening knobs

If your deploy needs more than the default open service, scryd offers two opt-ins:

- **`[server] require_peer_uid = true`** — recovers v0.2.0's `SO_PEERCRED` accept-time uid match. Requires every connecting peer to have the daemon's expected uid (typically the daemon's own, which means same-uid only). Mostly useful when scryd runs co-located with a single trusted process.
- **`[server] socket_mode = 0o660`** (or stricter) — combined with the operator placing the consumer's user in the `scryd` group, restricts socket access at the kernel layer.

Both are defense-in-depth, not the auth boundary. The auth boundary is your higher-layer API.

## Cross-references

- [`README.md`](../README.md) — install + use.
- [`scryd-spec.md`](../scryd-spec.md) — full §1-§6 spec.
- [`ops/README.install.md`](../ops/README.install.md) — install walkthrough.
