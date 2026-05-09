Parent slice(s): [build-and-packaging](../../slices/0.2.0/build-and-packaging.md), [security](../../slices/0.2.0/security.md), [cli](../../slices/0.2.0/cli.md), [migration](../../slices/0.2.0/migration.md)

# scryd v0.2.0 — Tasks

| #  | Task                                | Path                                                                                | Depends on   | Slice                                  |
|----|-------------------------------------|-------------------------------------------------------------------------------------|--------------|----------------------------------------|
| 00 | workspace-version-bump              | [00-workspace-version-bump.md](00-workspace-version-bump.md)                        | —            | (foundational)                         |
| 01 | peercred-allowed-uid                | [01-peercred-allowed-uid.md](01-peercred-allowed-uid.md)                            | —            | security                               |
| 02 | config-load-permission-preflight    | [02-config-load-permission-preflight.md](02-config-load-permission-preflight.md)    | —            | security                               |
| 03 | systemd-and-tmpfiles-templates      | [03-systemd-and-tmpfiles-templates.md](03-systemd-and-tmpfiles-templates.md)        | 01           | build-and-packaging                    |
| 04 | install-sh-system-rewrite           | [04-install-sh-system-rewrite.md](04-install-sh-system-rewrite.md)                  | 03           | build-and-packaging + migration        |
| 05 | uninstall-sh                        | [05-uninstall-sh.md](05-uninstall-sh.md)                                            | 04           | build-and-packaging                    |
| 06 | release-yml-linux-only              | [06-release-yml-linux-only.md](06-release-yml-linux-only.md)                        | 04, 05       | build-and-packaging                    |
| 07 | install-smoke-system-installer      | [07-install-smoke-system-installer.md](07-install-smoke-system-installer.md)        | 04, 05       | build-and-packaging + migration        |
| 08 | cli-sudo-path-resolution            | [08-cli-sudo-path-resolution.md](08-cli-sudo-path-resolution.md)                    | —            | cli                                    |
| 09 | cli-add-account-elevation           | [09-cli-add-account-elevation.md](09-cli-add-account-elevation.md)                  | 08           | cli                                    |
| 10 | cli-rotate-password                 | [10-cli-rotate-password.md](10-cli-rotate-password.md)                              | 09           | cli                                    |
| 11 | cli-remove-account                  | [11-cli-remove-account.md](11-cli-remove-account.md)                                | 09           | cli                                    |
| 12 | ops-readme-install-rewrite          | [12-ops-readme-install-rewrite.md](12-ops-readme-install-rewrite.md)                | 04, 05, 09   | build-and-packaging + migration (docs) |
| 13 | readme-rewrite                      | [13-readme-rewrite.md](13-readme-rewrite.md)                                        | 12           | build-and-packaging (docs)             |

## Dependency graph

```
00 workspace-version-bump   (foundational, no consumers gate on it)


              security tree
              ─────────────
01 peercred-allowed-uid ──┐
                          │
02 config-load-permission-preflight  (independent of every other task)


                            ops + migration tree
                            ────────────────────
                            │
01 ──> 03 systemd-and-tmpfiles-templates
        │
        ▼
       04 install-sh-system-rewrite ──┬─> 05 uninstall-sh ──┬─> 06 release-yml-linux-only
                                      │                     │
                                      │                     └─> 07 install-smoke-system-installer
                                      │
                                      └─────────────────────────────> 12 (also via 05, 09)


                  cli tree
                  ────────
08 cli-sudo-path-resolution
   │
   └─> 09 cli-add-account-elevation ──┬─> 10 cli-rotate-password
                                       └─> 11 cli-remove-account
                                       │
                                       └─────> 12 (also via 04, 05)


                  docs
                  ────
12 ops-readme-install-rewrite ──> 13 readme-rewrite
```

## Critical path

The longest dependency chain is the docs path:

```
01 → 03 → 04 → 05 → 12 → 13
```

Six tasks deep. 02 and 08 run fully independently from day 1; 09 / 10 / 11 chain off 08 in parallel with the build-and-packaging chain. 00 is independent.

## Per-slice allocation

| Slice                              | Tasks                       |
|------------------------------------|-----------------------------|
| build-and-packaging                | 03, 04, 05, 06, 07, 12, 13  |
| security                           | 01, 02                      |
| cli                                | 08, 09, 10, 11              |
| migration                          | 04 (shared), 07 (shared), 12 (shared) |
| (foundational)                     | 00                          |

Tasks 04, 07, and 12 span build-and-packaging and migration because the migration flows are bash code in the same files build-and-packaging owns; splitting them into separate task files would create a circular edit dance on the same scripts.

## What's NOT in v0.2.0

The v0.1.0 task pool at `tasks/` includes deferred work that v0.2.0 inherits without resolving:

- Task 12–15 (live IMAP client, fetch, idle/poll/tombstone, scheduler) — still deferred. v0.2.0's daemon-side changes (peercred env var, config-load preflight) compose with whatever lands later.
- Task 16 (runtime serve orchestration) — still deferred. v0.2.0's CLI prints `sudo systemctl start scryd` knowing the daemon is partial; once task 16 lands, the daemon under the new system unit becomes fully functional.
- Task 09 (witchcraft binding v2) — already done in the v0.1.0 cycle.

These are not v0.2.0 blockers; they are inherited work whose completion would compose cleanly with v0.2.0's install and CLI surface.
