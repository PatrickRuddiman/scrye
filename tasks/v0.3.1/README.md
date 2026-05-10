Parent plan: [v0.3.1 service pivot](C:\Users\prudd\.claude\plans\playful-yawning-bachman.md)

# scryd v0.3.1 — Tasks

| #  | Task                                | Path                                                                                | Depends on   | Layer                                       |
|----|-------------------------------------|-------------------------------------------------------------------------------------|--------------|---------------------------------------------|
| 00 | server-config-table                 | [00-server-config-table.md](00-server-config-table.md)                              | —            | scryd-config                                |
| 01 | peercred-opt-in                     | [01-peercred-opt-in.md](01-peercred-opt-in.md)                                      | 00           | scryd-api                                   |
| 02 | socket-bind-default-0666            | [02-socket-bind-default-0666.md](02-socket-bind-default-0666.md)                    | 00           | scryd-api                                   |
| 03 | drop-user-substitution              | [03-drop-user-substitution.md](03-drop-user-substitution.md)                        | 02           | ops                                         |
| 04 | drop-cli-elevation                  | [04-drop-cli-elevation.md](04-drop-cli-elevation.md)                                | 03           | scryd CLI                                   |
| 05 | account-ids-filter                  | [05-account-ids-filter.md](05-account-ids-filter.md)                                | —            | scryd-search + scryd-api + scryd CLI        |
| 06 | wire-witchcraft-indexer             | [06-wire-witchcraft-indexer.md](06-wire-witchcraft-indexer.md)                      | —            | scryd-runtime + scryd-search                |
| 07 | witchcraft-persistent-db            | [07-witchcraft-persistent-db.md](07-witchcraft-persistent-db.md)                    | 06           | scryd-runtime                               |
| 08 | weights-sha256-and-autofetch        | [08-weights-sha256-and-autofetch.md](08-weights-sha256-and-autofetch.md)            | 06, 07       | scryd-fetch-weights + scryd-runtime         |
| 09 | scheduler-completeness              | [09-scheduler-completeness.md](09-scheduler-completeness.md)                        | —            | scryd-imap                                  |
| 10 | cli-sync-status-autoreconcile       | [10-cli-sync-status-autoreconcile.md](10-cli-sync-status-autoreconcile.md)          | 09           | scryd CLI + scryd-api                       |
| 11 | tls-custom-ca-path                  | [11-tls-custom-ca-path.md](11-tls-custom-ca-path.md)                                | —            | scryd-imap                                  |
| 12 | docs-rewrite                        | [12-docs-rewrite.md](12-docs-rewrite.md)                                            | 01-11        | docs                                        |
| 13 | ci-and-e2e-refresh                  | [13-ci-and-e2e-refresh.md](13-ci-and-e2e-refresh.md)                                | 01-12        | tests + CI                                  |

## Dependency graph

```
                 architectural unwind
                 ────────────────────
00 server-config-table
   ├──> 01 peercred-opt-in
   └──> 02 socket-bind-0666 ──> 03 drop-__USER__ ──> 04 drop-cli-elevation


       search filter        witchcraft chain               scheduler        TLS
       ─────────────        ────────────────                ──────────       ───
       05 account_ids       06 wire-witchcraft              09 scheduler     11 tls-ca-path
                              │
                              ▼
                            07 persistent-db
                              │
                              ▼
                            08 weights sha + autofetch


                                              cli surface
                                              ───────────
                                              09 ──> 10 cli sync/status


                              {01-11} ──> 12 docs ──> 13 ci/e2e refresh
```

Critical path: `00 → 01/02 → 03 → 04` (architectural unwind, 4 deep) and `06 → 07 → 08` (witchcraft chain) parallelize. `05`, `09`, `11` are independent. `12` + `13` close on everything.

## Why this exists

v0.2.0 + v0.3.0 over-fitted scryd to "single Linux operator per host with kernel-level credential isolation between operator and daemon UID". v0.3.1 pivots to the service shape: install once per server, link N IMAP accounts, expose an open search API tagged by `account_id`, let the consumer's higher-layer API be the auth boundary. The bulk of the cycle unwinds the operator-isolation infrastructure (peercred default, socket group dance, sudo elevation, `__USER__` substitution) and finishes the genuinely-deferred items (witchcraft, scheduler, CLI sync/status).

## What's NOT in v0.3.1 — see `triage/` for the work-item notes

- OAuth / token-based IMAP auth → v0.3.5
- Auth at scryd's API layer → consumer's responsibility, by design
- OS keychain / HSM credential storage → service deploys lean on infra secret managers
- Indexing of attachment contents → still spec §3 Out
- macOS / Windows ports
- `uninstall.sh --dry-run`
- SysV-init / OpenRC / runit support
