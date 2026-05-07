Parent slice(s): [search-engine](../slices/search-engine.md), [multi-instance-isolation](../slices/multi-instance-isolation.md), [storage](../slices/storage.md), [imap-sync](../slices/imap-sync.md), [mime-and-markdown](../slices/mime-and-markdown.md), [api](../slices/api.md), [cli](../slices/cli.md), [build-and-packaging](../slices/build-and-packaging.md), [observability](../slices/observability.md)

# scryd — Tasks

| #  | Task                                | Path                                                          | Depends on    | Slice                                              |
|----|-------------------------------------|---------------------------------------------------------------|---------------|----------------------------------------------------|
| 00 | workspace-scaffold                  | [00-workspace-scaffold.md](00-workspace-scaffold.md)          | —             | build-and-packaging                                |
| 01 | scryd-config                        | [01-scryd-config.md](01-scryd-config.md)                      | 00            | build-and-packaging + observability                |
| 02 | scryd-log                           | [02-scryd-log.md](02-scryd-log.md)                            | 00            | observability                                      |
| 03 | scryd-storage-schema                | [03-scryd-storage-schema.md](03-scryd-storage-schema.md)      | 00            | storage                                            |
| 04 | scryd-storage-rw                    | [04-scryd-storage-rw.md](04-scryd-storage-rw.md)              | 03, 01        | storage                                            |
| 05 | scryd-storage-raw                   | [05-scryd-storage-raw.md](05-scryd-storage-raw.md)            | 03            | storage                                            |
| 06 | scryd-storage-tombstones-reindex    | [06-scryd-storage-tombstones-reindex.md](06-scryd-storage-tombstones-reindex.md) | 04 | storage |
| 07 | scryd-mime-parse-body               | [07-scryd-mime-parse-body.md](07-scryd-mime-parse-body.md)    | 00            | mime-and-markdown                                  |
| 08 | scryd-mime-attachments-faults       | [08-scryd-mime-attachments-faults.md](08-scryd-mime-attachments-faults.md) | 07 | mime-and-markdown                              |
| 09 | scryd-search-witchcraft-binding     | [09-scryd-search-witchcraft-binding.md](09-scryd-search-witchcraft-binding.md) | 00, 04 | search-engine                              |
| 10 | scryd-search-drainer                | [10-scryd-search-drainer.md](10-scryd-search-drainer.md)      | 09, 02        | search-engine                                      |
| 11 | scryd-search-query                  | [11-scryd-search-query.md](11-scryd-search-query.md)          | 09, 04        | search-engine                                      |
| 12 | scryd-imap-client                   | [12-scryd-imap-client.md](12-scryd-imap-client.md)            | 00, 01, 02    | imap-sync                                          |
| 13 | scryd-imap-fetch                    | [13-scryd-imap-fetch.md](13-scryd-imap-fetch.md)              | 12, 04, 05, 07 | imap-sync                                         |
| 14 | scryd-imap-idle-poll-tombstone      | [14-scryd-imap-idle-poll-tombstone.md](14-scryd-imap-idle-poll-tombstone.md) | 13, 06 | imap-sync                                  |
| 15 | scryd-imap-scheduler                | [15-scryd-imap-scheduler.md](15-scryd-imap-scheduler.md)      | 14            | imap-sync                                          |
| 16 | scryd-runtime-serve                 | [16-scryd-runtime-serve.md](16-scryd-runtime-serve.md)        | 02, 03, 15    | multi-instance-isolation + build-and-packaging     |
| 17 | scryd-api-uds                       | [17-scryd-api-uds.md](17-scryd-api-uds.md)                    | 16            | api + multi-instance-isolation                     |
| 18 | scryd-api-reads                     | [18-scryd-api-reads.md](18-scryd-api-reads.md)                | 17, 11, 04, 05 | api                                               |
| 19 | scryd-api-writes                    | [19-scryd-api-writes.md](19-scryd-api-writes.md)              | 18, 06, 15    | api                                                |
| 20 | scryd-cli-dispatch                  | [20-scryd-cli-dispatch.md](20-scryd-cli-dispatch.md)          | 00, 01, 16    | cli                                                |
| 21 | scryd-cli-search                    | [21-scryd-cli-search.md](21-scryd-cli-search.md)              | 20, 18        | cli                                                |
| 22 | scryd-cli-add-account               | [22-scryd-cli-add-account.md](22-scryd-cli-add-account.md)    | 20, 19        | cli                                                |
| 23 | scryd-cli-reindex                   | [23-scryd-cli-reindex.md](23-scryd-cli-reindex.md)            | 20, 19        | cli                                                |
| 24 | ops-systemd-unit-and-weights-fetcher| [24-ops-systemd-unit-and-weights-fetcher.md](24-ops-systemd-unit-and-weights-fetcher.md) | 16 | build-and-packaging                       |
| 25 | ops-install-script                  | [25-ops-install-script.md](25-ops-install-script.md)          | 24            | build-and-packaging                                |
| 26 | ops-ci-release-matrix               | [26-ops-ci-release-matrix.md](26-ops-ci-release-matrix.md)    | 25            | build-and-packaging                                |

## Dependency graph

```
                              00 workspace-scaffold
                              │
       ┌──────────────────────┼──────────────────────────────┬──────────────────────────────┐
       ▼                      ▼                              ▼                              ▼
   01 config              02 log                         03 storage-schema              07 mime-parse-body
       │                      │                              │                              │
       │                      │                              ├─> 04 storage-rw <────────┐   │
       │                      │                              │   (uses 01)              │   │
       │                      │                              │     │                    │   │
       │                      │                              │     ├─> 06 storage-tombstones-reindex
       │                      │                              │     │                        │
       │                      │                              ├─> 05 storage-raw              │
       │                      │                              │                               │
       │                      │                              │                               ├─> 08 mime-attachments-faults
       │                      │                              │                               │
       │                      │                              │                               │
       │                      │                              │  09 search-witchcraft-binding (00 + 04)
       │                      │                              │     │
       │                      │                              │     ├─> 10 search-drainer (09 + 02)
       │                      │                              │     │
       │                      │                              │     └─> 11 search-query (09 + 04)
       │                      │                              │
       │                      ├──────────────────────────────┴───────┐
       │                      │                                      │
       └──> 12 imap-client (00 + 01 + 02)                            │
              │                                                       │
              └─> 13 imap-fetch (12 + 04 + 05 + 07)                   │
                     │                                                 │
                     └─> 14 imap-idle-poll-tombstone (13 + 06)         │
                            │                                          │
                            └─> 15 imap-scheduler (14)                 │
                                   │                                    │
                                   └─> 16 runtime-serve (02 + 03 + 15) │
                                          │                             │
                                          ├─> 17 api-uds (16) ──────────┤
                                          │      │                       │
                                          │      └─> 18 api-reads (17 + 11 + 04 + 05)
                                          │             │
                                          │             └─> 19 api-writes (18 + 06 + 15)
                                          │                    │
                                          │                    ├─> 22 cli-add-account (20 + 19)
                                          │                    └─> 23 cli-reindex (20 + 19)
                                          │
                                          ├─> 20 cli-dispatch (00 + 01 + 16)
                                          │      │
                                          │      └─> 21 cli-search (20 + 18)
                                          │
                                          └─> 24 ops-systemd-unit-and-weights-fetcher (16)
                                                 │
                                                 └─> 25 ops-install-script (24)
                                                        │
                                                        └─> 26 ops-ci-release-matrix (25)
```

## Critical path

The longest dependency chain is the daemon-runtime path:

```
00 → 01 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 22
```

11 tasks deep. Tasks outside that chain (02 log, 03–06 storage non-rw, 07–08 mime, 09–11 search, 20–23 cli, 24–26 ops) can be parallelized once their immediate predecessors land.

## Per-slice task allocation

| Slice                       | Tasks                  |
|-----------------------------|------------------------|
| build-and-packaging         | 00, (01), (16), 24, 25, 26 |
| observability               | (01), 02               |
| storage                     | 03, 04, 05, 06         |
| mime-and-markdown           | 07, 08                 |
| search-engine               | 09, 10, 11             |
| imap-sync                   | 12, 13, 14, 15         |
| multi-instance-isolation    | (16), (17)             |
| api                         | (17), 18, 19           |
| cli                         | 20, 21, 22, 23         |

Parenthesized tasks span multiple slices and are listed in each.
