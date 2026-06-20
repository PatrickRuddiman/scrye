# scryd security posture

scryd is a single-purpose daemon that serves **one mailbox** over a **loopback-only MCP server**. Its security model is intentionally small: bind nothing routable, serve only the mailbox named by `USER_EMAIL`, and keep the IMAP credential readable only by the dedicated `scryd` system user. This document spells out exactly what scryd defends against and what it doesn't.

## The boundary: `USER_EMAIL` + loopback

scryd has two structural defenses, and they are the whole boundary:

1. **Loopback-only bind.** The MCP server binds `127.0.0.1:7878` by default and refuses any non-loopback address (validated at startup; `SCRYD_MCP_BIND` may move the port but must stay on a loopback interface). Nothing off-host can reach it.
2. **Single-mailbox scoping.** `USER_EMAIL` is mandatory. At startup the daemon keeps only the configured `[[accounts]]` whose `user` (IMAP login) equals `USER_EMAIL` (case-insensitive); it fetches and indexes that account only. Every MCP tool re-resolves the owned `account_id`(s) on each call and filters results to them. A message id from any other account returns `not_found` — the same answer a missing id gets, so existence never leaks. If `USER_EMAIL` is unset/empty the daemon exits non-zero before binding anything.

## What scryd defends against

- **IMAP credential at rest is not world-readable.** The config file is mode `0640` owned by `scryd:scryd`. A non-root user not in the `scryd` group cannot read it.
- **Read-only against IMAP.** scryd's command set is structurally restricted to the read-only IMAP verb allow-list (`EXAMINE`, `UID FETCH`, `UID SEARCH`, `IDLE`, `NOOP`, `LOGOUT`, `CAPABILITY`). It cannot send, delete, flag, or move messages even if compromised.
- **Indexed content is not executable.** scryd parses MIME, extracts text + Markdown body, indexes them. It does not render HTML, execute scripts, or open attachments.
- **TLS to the IMAP server is real.** Default `webpki-roots` trust anchors; per-account `tls_ca_path` for self-signed corporate CAs. There is no skip-verify option.
- **Off-host reach is impossible by construction.** The MCP listener is loopback-only; the daemon binds no Unix socket and exposes no routable port.
- **Logged stderr is structured + scrubbed.** Credentials never enter the log line; the `AccountPassword` type's `Debug` impl prints `[REDACTED]`.
- **No write surface.** The MCP server advertises six read-only tools. There is no tool to add accounts, trigger syncs, rewrite the index, or change configuration — fetch and index are internal, automatic behaviors.

## What scryd does NOT defend against

- **Other local processes on the same host.** Any process that can reach `127.0.0.1:7878` can call the read-only tools and read the mailbox. v1 has no per-caller authentication; the host is the trust boundary. Run scryd where only trusted local agents run (a personal workstation, or a server whose local users you trust).
- **A misconfigured `USER_EMAIL`.** Whatever address it is set to defines the served mailbox. Point it at the wrong account and the wrong account is served.
- **Root on the host.** Root reads the config file, the witchcraft sqlite, and the daemon's process memory verbatim.
- **Side-channels in the search response.** Hit counts, snippets, and `account_id` are visible to any local caller. Don't run scryd on a shared host where the existence of the mailbox is itself a secret.

## Operating scryd safely

1. **Keep it loopback.** Do not put scryd behind a reverse proxy that exposes `127.0.0.1:7878` to a network. If you must reach it remotely, tunnel over SSH to localhost rather than rebinding it.
2. **Run it on a single-trust host.** The MCP server has no auth in v1; treat any local process as able to read the mailbox.
3. **Set `USER_EMAIL` deliberately** and confirm the matching `[[accounts]]` entry's `user` equals it, so the daemon serves exactly the mailbox you intend.
4. **Restrict the config.** Leave the config file mode at `0640 scryd:scryd`; only add admins to the `scryd` group if they should read the IMAP credential.

## Future work (not in v1)

- **Bearer-token auth on the MCP endpoint.** The architecture admits a future `[server] auth = "bearer"` so a per-caller token can gate the tools. Until then, loopback + single-mailbox scoping is the boundary.

## Cross-references

- [`README.md`](../README.md) — install + use.
- [`scryd-spec.md`](../scryd-spec.md) — full §1–§6 spec.
- [`ops/README.install.md`](../ops/README.install.md) — install walkthrough.
