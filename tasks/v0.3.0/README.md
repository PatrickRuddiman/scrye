Parent plan: GreenMail integration testing for scryd

# scryd v0.3.0 — Tasks

| #  | Task                                | Path                                                                                | Depends on   | Layer                                  |
|----|-------------------------------------|-------------------------------------------------------------------------------------|--------------|----------------------------------------|
| 00 | greenmail-fixture-support           | [00-greenmail-fixture-support.md](00-greenmail-fixture-support.md)                  | —            | test infrastructure                    |
| 01 | accountcfg-tls-bool                 | [01-accountcfg-tls-bool.md](01-accountcfg-tls-bool.md)                              | —            | scryd-config + scryd-imap              |
| 02 | imap-fetch-parsing                  | [02-imap-fetch-parsing.md](02-imap-fetch-parsing.md)                                | 00, 01       | scryd-imap                             |
| 03 | imap-initial-and-incremental        | [03-imap-initial-and-incremental.md](03-imap-initial-and-incremental.md)            | 02           | scryd-imap                             |
| 04 | imap-idle-loop                      | [04-imap-idle-loop.md](04-imap-idle-loop.md)                                        | 03           | scryd-imap                             |
| 05 | imap-poll-loop                      | [05-imap-poll-loop.md](05-imap-poll-loop.md)                                        | 03           | scryd-imap                             |
| 06 | imap-tombstone-scan                 | [06-imap-tombstone-scan.md](06-imap-tombstone-scan.md)                              | 03           | scryd-imap                             |
| 07 | imap-scheduler-supervisor           | [07-imap-scheduler-supervisor.md](07-imap-scheduler-supervisor.md)                  | 04, 05, 06   | scryd-imap                             |
| 08 | runtime-serve                       | [08-runtime-serve.md](08-runtime-serve.md)                                          | 07           | scryd-runtime + scryd bin              |
| 09 | e2e-imap-to-search                  | [09-e2e-imap-to-search.md](09-e2e-imap-to-search.md)                                | 08           | CI + tests/                            |

## Dependency graph

```
00 greenmail-fixture-support ──┐
                               ├──> 02 imap-fetch-parsing ──> 03 imap-initial-and-incremental ──┬──> 04 imap-idle-loop ──┐
01 accountcfg-tls-bool ────────┘                                                                ├──> 05 imap-poll-loop ──┤
                                                                                                └──> 06 tombstone-scan ──┤
                                                                                                                         ▼
                                                                                                    07 scheduler-supervisor
                                                                                                                         │
                                                                                                                         ▼
                                                                                                          08 runtime-serve
                                                                                                                         │
                                                                                                                         ▼
                                                                                                          09 e2e-imap-to-search
```

## Critical path

`00 → 02 → 03 → 07 → 08 → 09`. 04, 05, 06 fan out from 03 and parallelize. 01 sits independently and merges with 00 at 02.

## Why this exists

The v0.1.0 task files at `tasks/13-…md` through `tasks/16-…md` already specify the IMAP fetch / IDLE / poll / scheduler / runtime-serve work, but every live-orchestration bullet in those files is marked **Deferred — needs a mock IMAP server harness**. Rather than hand-rolling a Rust IMAP test server (which the task notes would be its own large task), v0.3.0 takes the cheaper path: spin up [GreenMail](https://greenmail-mail-test.github.io/greenmail/) (`greenmail/standalone` Docker image) as the test backbone and land the deferred bullets against a real IMAP server. The end state is a true `inject mail → daemon indexes → scryd search returns hits` e2e test.

## What's NOT in v0.3.0

- IMAPS / TLS verification path against the test fixture. v0.3.0 introduces a `tls: bool` field on `AccountCfg` (defaults `true`) so tests can opt into plain-IMAP on port 3143; production accounts keep `tls = true` and exercise the existing `webpki-roots` path.
- macOS / Windows ports. Linux-only release pipeline (established by v0.2.0).
- Multi-tenant filtering / multi-operator support. Single-operator per host stays the model (v0.2.0 confirmed).
- Performance benchmarks. GreenMail's latency profile is artificial; spec §4 quality bars apply against production IMAP deployments.
