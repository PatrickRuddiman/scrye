Parent spec: [scryd-spec.md](../scryd-spec.md)

Supersedes: [api](api.md) — this slice replaces the HTTP-over-Unix-socket surface
with an MCP server. The `api.md` slice is retained for history; the `scryd-api`
crate it describes is removed by this slice's tasks.

> **Implementation note (post-merge):** §1 and Phase B2 below describe rewiring
> the `scryd` CLI into an MCP client. That is **not** what shipped. Per a later
> directive the daemon was reduced to *exactly* fetch + index + MCP-serve with
> **no other control surface or client**, so the `scryd` CLI client was **removed
> entirely** rather than rewired — `scryd` is now a subcommand-less daemon and
> search is reachable only over the MCP server. The MCP server design in this
> slice is otherwise accurate to the code. See [cli.md](cli.md) (superseded) and
> the root [README.md](../README.md).

# scryd — mcp

## §1 Summary

Replaces scryd's caller-facing surface with a **Model Context Protocol (MCP)
server** so any local MCP-capable AI client can drive a running scryd instance.
The server speaks **MCP over Streamable HTTP on loopback TCP** (`127.0.0.1:<port>`,
default `7878`) and takes a **mandatory `USER_EMAIL` environment variable** that
**scopes every response to that address**: the server only ever returns data for
the account(s) whose IMAP login (`accounts.username`) equals `USER_EMAIL`
(case-insensitive). The nine operations the `api` slice exposed as HTTP routes
become nine MCP **tools**. The daemon's storage / search / scheduler / crash-loop
health (issue #20) are unchanged — only the transport and the auth/scoping
boundary change. The `scryd` CLI is rewired from a UDS HTTP client to an MCP
streamable-HTTP client.

## §2 Codebase reconnaissance

Brownfield change. What exists today and what each task must reconcile with:

- **`scryd-api`** (axum 0.7, removed by this slice): `router.rs` (9 routes +
  access-log middleware), `handlers.rs` (search/message/raw/thread/accounts/
  status read logic + `derive_snippet`/`filter_match`), `handlers_write.rs`
  (sync/reindex/reconcile), `dto.rs` (request/response shapes; `LIMIT_DEFAULT=20`,
  `LIMIT_MAX=200`, snippet 240 chars), `state.rs` (`AppState` +
  `DaemonHealthSnapshot` + `ReindexHandle`), `serve`/`bind` (UDS listener,
  `SO_PEERCRED`). The business logic in the handlers is **ported**, not rewritten.
- **`scryd-runtime::serve`** (`serve.rs`): builds `AppState`, calls
  `scryd_api::bind` on `$XDG_RUNTIME_DIR/scryd/scryd.sock` then `scryd_api::serve`.
  Lines ~166–195 are the transport block this slice swaps. Everything else
  (`begin_run`/`crash_backoff`/`finish_run`, `daemon_health`, scheduler, drainer,
  weights) stays. `ServeContext` carries the api handle + shutdown channel —
  swapped for the MCP server handle + cancellation token.
- **`scryd` CLI** (`scryd/src/main.rs`, `uds_client.rs`, `path_resolution.rs`,
  `output.rs`): verbs `search`/`status`/`sync`/`reindex` call `UdsClient.get/post`
  on the old routes; `add-account`/`rotate-password`/`remove-account` write
  config then `POST /internal/reconcile`. `uds_client.rs` + socket path resolution
  are removed; an MCP client replaces them. `output.rs` rendering of search hits
  is retained, fed from the `search` tool's structured result.
- **Account model**: `scryd-config::AccountCfg.user` is the IMAP login (the user's
  email); `scryd-storage::AccountRow.username` is the mirrored copy.
  `StorageHandle::list_active_accounts()` / `list_all_accounts()` are the lookup.
  `MessageRow.account_id` ties every message to its account. This is the join that
  powers `USER_EMAIL` scoping.

External crates this slice leans on:

- **`rmcp` 1.7** — the official MCP Rust SDK. Server features
  `server, macros, schemars, transport-streamable-http-server`; client features
  for the CLI (reqwest-based streamable-HTTP client). Verified API:
  - `#[tool_router] impl Server { #[tool(description="…")] async fn x(&self,
    Parameters(args): Parameters<Args>) -> Result<CallToolResult, McpError> {…} }`
    where `Args: serde::Deserialize + schemars::JsonSchema`.
  - `#[tool_handler] impl ServerHandler for Server { fn get_info(&self) ->
    ServerInfo {…} }`.
  - Transport: `StreamableHttpService::new(|| Ok(Server::new(state)),
    LocalSessionManager::default().into(), StreamableHttpServerConfig::default())`,
    mounted via `axum::Router::new().nest_service("/mcp", service)`, served by
    `axum::serve(TcpListener::bind(addr), router).with_graceful_shutdown(…)`.
  - `rmcp` 1.7 pulls **axum 0.8 / hyper 1 / tokio-util 0.7**. Removing `scryd-api`
    removes the only axum-0.7 user; confirm exact transitive versions with
    `cargo tree -p rmcp` before pinning.
- **`axum` 0.8**, **`tokio`**, **`serde`/`serde_json`**, **`schemars`** (tool JSON
  schemas), **`tokio-util`** (`CancellationToken` for graceful shutdown).

## §3 Decisions

1. **Transport = MCP over Streamable HTTP on loopback TCP.** Bind
   `127.0.0.1:<port>` only — never `0.0.0.0`. Mounted at path `/mcp`. Rationale:
   MCP is the requested surface; Streamable HTTP is `rmcp`'s supported HTTP
   transport and lets any local MCP client (and the rewired CLI) connect. No TLS
   (loopback); no remote exposure.
2. **`USER_EMAIL` is mandatory and scopes everything.** Read once at daemon
   startup; if unset/empty the daemon **fails fast** (non-zero exit, clear config
   error) and never serves. Rationale: the server's contract is "all responses are
   for this address" — serving unscoped because the var is missing would silently
   violate it.
3. **Scoping = account ownership, matched on `accounts.username`
   (case-insensitive).** `USER_EMAIL` resolves to the set of `account_id`s whose
   `username == USER_EMAIL`. Resolved **per tool call** (a fast indexed read of
   active accounts) so newly added/removed accounts are reflected without restart.
   Rationale: the defensible "my mailbox" boundary for an AI assistant; not a
   From/To address filter. An empty allowed-set (no configured account matches)
   means every tool returns empty/`not_found` — never an error that leaks "you
   have no such account".
4. **Per-tool scope enforcement is mandatory and uniform.**
   - `search`: the allowed account_ids are forced into the search query's account
     filter. A caller-supplied `account_ids` may only **intersect** (narrow),
     never widen, the allowed set.
   - `get_message` / `get_raw_message`: fetch the row, then return `not_found`
     when `row.account_id ∉ allowed` — indistinguishable from a missing id (no
     existence leak).
   - `get_thread`: drop messages whose `account_id ∉ allowed`; if none remain,
     `not_found`.
   - `list_accounts` / `status` (per-account block) / `sync`: only allowed
     accounts appear / are signaled.
   - `reconcile`: the returned `accounts_added/updated/inactivated` lists are
     filtered to allowed ids.
   - `reindex`: global rebuild; the response is `{started}` only — carries no
     per-account data, so it stays scope-safe while remaining a daemon control.
5. **Tools return structured JSON content.** Each tool serializes a DTO (the
   existing `dto.rs` shapes, ported) into MCP structured content so MCP clients
   and the CLI get typed results. Rationale: preserves the wire shapes the CLI/
   `jq` users already know; MCP clients still get schema'd output.
6. **Error mapping to `McpError`.** Invalid arguments (bad `mode`, malformed
   `since`/`until` date) → `invalid_params`. Missing / out-of-scope id →
   `not_found`. Backend failure → `internal_error` (body suppresses internals,
   full context logged). A reindex requested while one is in flight →
   `internal_error`/conflict-coded tool error (single-flight preserved). Rationale:
   mirrors the old slice's status-code map onto MCP's error model.
7. **`scryd-api` is retired; a new `scryd-mcp` crate owns the surface.**
   `McpState` mirrors `AppState` (storage, searcher, indexer, reindex_lock,
   config, started_at, scheduler, daemon_health) plus the resolved scope source.
   `DaemonHealthSnapshot` and `ReindexHandle` move into `scryd-mcp`. Rationale:
   reuse all business logic; replace only transport + scoping; keep the issue-#20
   crash-loop health surface intact (now exposed via the `status` tool).
8. **Config: `[server]` becomes `{ mcp_bind: String }`.** Default
   `"127.0.0.1:7878"`. `require_peer_uid` and `socket_mode` are removed (UDS-only
   concepts). Env `SCRYD_MCP_BIND` overrides config; `USER_EMAIL` is env-only
   (never config — it's a per-process scoping credential). Old keys in existing
   config files are ignored (serde default) and called out in the README
   migration note. Rationale: loopback bind address is the only server knob the
   MCP transport needs.
9. **Local-trust posture is documented, not enforced in v1.** Loopback TCP is
   reachable by any local process of any local user (unlike UDS + `SO_PEERCRED`).
   The prior API already defaulted to open (`require_peer_uid=false`), so this is a
   comparable posture; `USER_EMAIL` scoping bounds the blast radius to one
   mailbox. A bearer token on `/mcp` is a noted future follow-up, out of scope
   here. Rationale: keep v1 simple and shippable; make the tradeoff explicit.
10. **CLI becomes an MCP client.** Verbs map 1:1 to tool calls over
    `http://<mcp_bind>/mcp`; bind resolved from `SCRYD_MCP_BIND`/config/default.
    `DaemonNotRunning` is reported on connection-refused (replacing "no socket").
    Rationale: the CLI stays the operator entry point with identical UX, over the
    new transport.
11. **Tools only (no MCP resources/prompts) in v1.** Rationale: the nine
    operations are actions; tools are the right MCP primitive. Resources/prompts
    are a possible v2.

## §4 Contracts & shapes

New internal crate: `scryd-mcp`. MCP server name/version from build env;
`ServerInfo` advertises `tools` capability and an instructions string naming the
nine tools.

Tool catalogue (input → output DTO; all outputs scoped per §3.4):

- **`search`** — in: `q` (string, default `""`), `from?`, `since?`/`until?`
  (`YYYY-MM-DD`), `folder?`, `account_ids?` (narrow-only), `limit?` (1..=200,
  default 20), `mode?` (`fulltext`|`semantic`|`hybrid`, default `fulltext`). out:
  `{ hits: [{message_id, account_id, folder, sender_addr, sender_name?, subject?,
  date(ISO-8601), score, snippet, thread_id}], mode, elapsed_ms }`. Snippet ≤ 240
  chars; `derive_snippet`/`filter_match` ported from `handlers.rs`.
- **`get_message`** — in: `{ id }`. out: full parsed-message DTO (`from`/`to`/`cc`
  as `{addr,name?}`, `attachments` `[{filename,mime_type,size_bytes}]`, `body_md`,
  `references[]`, `thread_id`, …). `not_found` if tombstoned/missing/out-of-scope.
- **`get_raw_message`** — in: `{ id }`. out: `{ message_id, raw }` (RFC 5322
  source). Same scope guard.
- **`get_thread`** — in: `{ id }`. out: `{ thread_id, messages: [parsed-message,…]
  }` oldest-first, filtered to allowed accounts; `not_found` if empty after
  filtering.
- **`list_accounts`** — in: none. out: `{ accounts: [{account_id, folders[]}] }`
  (allowed only).
- **`status`** — in: none. out: `{ ok, uptime_secs, accounts: [{account_id,
  folders, health?, last_sync_unix?, last_seen_uid?}] (allowed only), drainer:
  {queue_depth, failed_permanent, last_index_error?}, daemon: {restart_count,
  consecutive_crashes, last_crash_unix?, in_crash_loop, in_backoff,
  backoff_until_unix?} }`. `ok = !in_crash_loop` (issue #20).
- **`sync`** — in: none. out: `{ started: true, scope: { accounts: [allowed…] } }`
  (signals scheduler for allowed accounts only).
- **`reindex`** — in: none. out: `{ started: true }`. Single-flight via
  `Mutex<Option<ReindexHandle>>`; concurrent call → conflict tool-error.
- **`reconcile`** — in: none. out: `{ started: true, accounts_added[],
  accounts_updated[], accounts_inactivated[] }` filtered to allowed ids.

Scope helper contract (`AccountScope`): `from_env() -> Result<Self, ConfigError>`
(reads `USER_EMAIL`, errors if unset/empty); `allowed_account_ids(&StorageHandle)
-> Result<Vec<String>>` (case-insensitive `username == USER_EMAIL`);
`is_allowed(&allowed, account_id) -> bool`; `retain_allowed(&allowed, rows)`;
`narrow(&allowed, caller_ids) -> Vec<String>` (intersection).

Transport contract (`scryd-mcp::serve`): given `McpState` + bind `SocketAddr` +
a shutdown future, build the `StreamableHttpService`, `nest_service("/mcp", …)`,
and `axum::serve` on a loopback `TcpListener` with graceful shutdown.

## §5 Sequence / wiring

Daemon (`scryd-runtime::serve::serve_init`), replacing the UDS block:

```
read USER_EMAIL  ──fail-fast if missing──> RuntimeError::Config
… begin_run / crash_backoff / daemon_health  (unchanged, issue #20) …
build McpState { storage, searcher, indexer, reindex_lock, config,
                 started_at, scheduler, daemon_health, scope }
resolve mcp_bind (SCRYD_MCP_BIND | config.server.mcp_bind | 127.0.0.1:7878)
spawn scryd_mcp::serve(state, bind, shutdown_rx)   // replaces bind()+scryd_api::serve()
log STARTUP ; ServeContext { …, mcp_handle, mcp_shutdown, storage, run_id }
serve_run: signal mcp_shutdown → await handle → scheduler.shutdown → drainer cancel
           → finish_run(run_id)  (unchanged)
```

CLI (`scryd/src/main.rs`): each verb opens an MCP client to
`http://<mcp_bind>/mcp`, calls the matching tool, deserializes the structured
result, renders as today (`output.rs` for `search`; raw JSON for `status`).

## §6 Out of scope

- Bearer-token / OAuth auth on `/mcp` (future).
- Multi-user tenancy (one `USER_EMAIL` per daemon process).
- Any change to IMAP sync, storage schema, the witchcraft indexer, MIME parsing,
  or the issue-#20 crash-loop/backoff logic.
- MCP **resources** and **prompts** (tools only in v1).
- Rate limiting (carried over from `api` §3.17 — still local-only).

## §7 Open questions

None — all resolved:

1. **CLI client transport feature.** Resolved: the CLI enables `rmcp`'s reqwest
   streamable-HTTP **client** feature; the exact feature-flag string is confirmed
   by `cargo` during the CLI-rewire task (a build-time lookup, not a design
   choice) and is not a blocker.
2. **Bearer token in v1.** Resolved: **no** auth token in v1. Loopback-only bind
   plus `USER_EMAIL` account-scoping is the boundary (§3.9); a token is a
   documented future follow-up, out of scope (§6).
3. **`reindex`/`reconcile` exposure.** Resolved: **exposed as tools** but kept
   scope-safe (§3.4 — `reindex` returns only `{started}`; `reconcile` filters
   account lists), so the CLI and generic MCP clients share one surface.
