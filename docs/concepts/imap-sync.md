---
sources:
  - crates/scryd-imap/src/scheduler.rs
  - crates/scryd-imap/src/supervisor.rs
  - crates/scryd-imap/src/idle.rs
  - crates/scryd-imap/src/poll.rs
  - crates/scryd-imap/src/fetch.rs
  - crates/scryd-imap/src/connect.rs
  - crates/scryd-imap/src/client.rs
  - crates/scryd-imap/src/capabilities.rs
  - crates/scryd-imap/src/verbs.rs
  - crates/scryd-imap/src/tombstone.rs
  - crates/scryd-imap/src/uidvalidity.rs
  - crates/scryd-imap/src/tls.rs
---

# IMAP sync

scryd keeps a local copy of mail by connecting to your IMAP provider, backfilling
history once, then watching each folder for new messages. This page describes how
that runs and what the daemon is allowed to do over IMAP.

## Scheduler and supervisors

The scheduler owns a tree of supervisors, one per (account, folder) pair. It
starts a supervisor for every folder of every retained account and reconciles
that tree as configuration changes: a pair that disappears from config has its
supervisor signaled to exit, and a new pair gets a new supervisor.

Which accounts run is decided by scope, not by config size alone: only accounts
whose login equals `USER_EMAIL` are served. Which folders run comes from the
per-account `folders` override, falling back to the `[sync].folders` default. See
[Configuration](../configuration.md) for both keys.

Each account opens at most `MAX_CONNECTIONS_PER_ACCOUNT` (5) concurrent IMAP
connections, a conservative floor under common provider session caps.

## Per-supervisor cycle

A supervisor runs one cycle at a time:

1. **Log in** to the account over TLS or plaintext (below).
2. **Clear stale health** by marking the account `active` once authentication
   succeeds, so a fixed credential clears a prior `auth-rejected` state.
3. **Backfill** the folder's history once (`run_initial_backfill`).
4. **Watch** the folder for new mail with an IDLE loop (`run_idle_loop`).

When a cycle returns cleanly the supervisor waits briefly to avoid a hot
reconnect; when it fails it waits out an exponential backoff before retrying.

### Connection security

Each account connects over TLS or plaintext according to its `tls` flag, with an
optional `tls_ca_path` to trust a private CA in place of the bundled web PKI
roots. The `tls` flag chooses TLS or plaintext — it does not disable certificate
verification. See [Configuration](../configuration.md) and
[Security](../security.md).

### IDLE loop

The IDLE loop follows RFC 2177: enter IDLE, wait for the server to signal new or
changed mail, send `DONE`, run an incremental fetch, then re-enter IDLE. The
channel is recycled every `IDLE_RECYCLE` (25 minutes) to survive servers that
close idle connections early. The loop also runs a periodic tombstone scan (see
below) on a `TOMBSTONE_SCAN_EVERY` cadence.

### Backoff on failure

Repeated connection failures back off exponentially. `backoff_after` starts at
`INITIAL_BACKOFF` (30 seconds) and doubles per consecutive failure, capped at
`BACKOFF_CAP` (3600 seconds): 30s, 60s, 120s, … up to one hour.

### The poll path is present but unwired

A polling loop (`run_poll_loop`, with a `POLL_INTERVAL_FLOOR` of 30 seconds)
exists and is tested, but the shipped supervisor does not call it: every cycle
watches with IDLE regardless of configuration. The `[sync].poll_interval_seconds`
and `[sync].use_idle` keys are parsed and validated but not consulted by the
running daemon. See [Configuration](../configuration.md), which documents this
honestly alongside the keys.

## UID validity and tombstones

scryd tracks each folder's `UIDVALIDITY`. When the server resets it, the stored
UID watermark is no longer meaningful, so scryd re-syncs the folder from the
start (`uidvalidity.rs`). A separate tombstone scan reconciles deletions: mail
that has disappeared server-side is marked removed locally (`tombstone.rs`).

## Capabilities

On connect scryd reads the server's advertised capabilities (`capabilities.rs`)
and uses them to decide what is available (for example, whether IDLE is
supported). It never issues a command outside the allow-list below.

## IMAP verb allow-list

scryd is read-only against your mailbox. The set of IMAP verbs it may issue is a
closed, compile-time-exhaustive enumeration (`verbs.rs`). Any verb not on this
list is rejected with `MutatingCommandRejected` (and panics in debug builds), so
a future change cannot route a mutating command through without also editing the
allow-list:

| Verb | Verb | Verb | Verb |
| --- | --- | --- | --- |
| `Capability` | `Login` | `Authenticate` | `Logout` |
| `Examine` | `List` | `Lsub` | `Status` |
| `Fetch` | `UidFetch` | `Search` | `UidSearch` |
| `Idle` | `Done` | `Noop` | `Id` |
| `Enable` | `GetMetadata` | `GetAcl` | `GetQuota` |
| `GetQuotaRoot` | | | |

Every verb is read-only. Notably, the folder is opened with `Examine` (read-only)
rather than `Select`, and there is no `Store`, `Copy`, `Move`, `Expunge`,
`Append`, `Create`, `Delete`, or `Rename`.

## Account health

Each cycle records a connection-health string for the (account, folder) pair,
surfaced in the MCP [`status`](../mcp/tools/status.md) tool's `health` field. The
values are a closed set:

| Value | Meaning |
| --- | --- |
| `active` | Authenticated and syncing. |
| `auth-rejected` | The server rejected the credentials. |
| `connect-failure` | The connection could not be established. |
| `tls-failure` | The TLS handshake failed. |
| `transient` | A temporary error; the supervisor will retry. |

## See also

- [Architecture](architecture.md)
- [Configuration](../configuration.md)
- [Storage](storage.md)
- [Indexing and search](indexing-and-search.md)
- [status tool](../mcp/tools/status.md)
