Parent: [scryd](scryd-spec.md)

# scryd v0.2.0 — Local Credential Isolation

## §1 Summary

scryd indexes a Linux user's email and exposes that index to local programs over a per-user search interface. v0.2.0 strengthens the v0.1.0 contract with a single hard property: programs running as the same Linux user as the operator — the operator's shell, scripts, AI agents, MCP servers, and any other tool launched from the operator's account — shall not be able to read the stored IMAP credential. The means by which the property holds is the operating system's per-account access control: the daemon runs as a Linux account distinct from the operator's, and the credential at rest and in memory belongs to that distinct account. Local programs running as the operator can still call the daemon's search interface; they cannot read the file the daemon stores the credential in, and they cannot inspect the daemon's process memory.

Success criteria:

- A program running under the operator's Linux account, given thirty minutes to attempt any sequence of standard system calls and any combination of standard tooling on the host, recovers the IMAP credential in zero of those minutes.
- A search query the operator issues from a terminal in their normal account returns its first result in under two hundred milliseconds at the 95th percentile and under five hundred milliseconds at the 99th percentile, on a corpus of ten thousand messages with warm cache.
- A different Linux user on the same host who is not granted explicit access by the operator or by root can issue zero commands that interact with the operator's scryd, observe the operator's mail, or read the operator's IMAP credential.
- An end-to-end install (acquire the release artifact, run the install command, configure one IMAP account, run a search that returns an indexed message) completes in under five minutes on a freshly-installed Linux host with a 100 Mbit/s connection, including the one-time download of the model weights.

## §2 Behavior

### Personas

**Operator.** A Linux user who owns the mail account and intends to index it. Recurring tasks: install scryd on the host, configure one or more IMAP accounts, rotate an IMAP credential when the upstream provider rotates it, upgrade scryd to a new release within the v0.2.x series, remove a configured IMAP account, uninstall scryd, search their mail interactively from a terminal, retrieve the body of a specific message, retrieve a thread, list configured accounts, trigger a manual sync, trigger a reindex, inspect daemon health and recent failures.

**Local agent.** A program running under the operator's Linux account that the operator has chosen to delegate work to. Recurring tasks: search the operator's mail, retrieve a message by identifier, retrieve a thread by identifier, list the operator's configured accounts (without secrets), trigger a sync, trigger a reindex, fail loudly when scryd is not running so that the operator notices.

**Adversary on the operator's account.** A program that has acquired execution privilege equivalent to the operator's Linux account, against the operator's intent. Examples include a prompt-injected agent, a compromised dependency in the operator's tooling, and a malicious script the operator was tricked into running. Its recurring task is one task: recover the IMAP credential and exfiltrate it. v0.2.0 promises this task fails.

**Unrelated Linux user.** A different login on the same host. Their recurring tasks are unrelated to scryd. Their interaction with scryd is the absence of one: scryd shall neither expose information to them nor reduce the security of their account.

### User stories

- As an *operator*, I want to install scryd in a single elevated command, so that my mail starts indexing and my unprivileged terminal account gains a search command afterward.
- As an *operator*, I want to configure an IMAP account by running an elevated command that prompts for the credential interactively, so that I do not need to edit configuration files by hand and the credential never appears in my shell history.
- As an *operator*, I want to rotate an IMAP credential when my mail provider rotates it, without losing the existing index and without forcing a reindex.
- As an *operator*, I want to upgrade scryd from one v0.2.x release to the next, preserving my account configuration, my index, and my downloaded weights.
- As an *operator*, I want to remove a configured IMAP account, after which scryd stops syncing it and the indexed messages from that account are no longer returned by search.
- As an *operator*, I want to uninstall scryd, after which every artifact the install produced is removed and no scryd process continues to run.
- As an *operator*, I want to search my mail in plain English from a terminal in my normal user account, with no elevation, getting ranked results that cite the source message.
- As an *operator*, I want to retrieve the full body of a message scryd has indexed, given the message identifier from a search result.
- As an *operator*, I want a named report when scryd cannot reach my IMAP server, when authentication fails, or when the upstream provider rate-limits sync, so that I know my index is going stale and I know what to do about it.
- As a *local agent*, I want to query the operator's mail using the same interface the operator uses, returning the same result shape, so that the operator does not maintain two integrations.
- As a *local agent*, I want my attempts to read the IMAP credential — by file, by process memory, by environment variable, or by any other means a same-account process can normally use — to fail at the operating-system layer, so that even when I am subverted I cannot exfiltrate the credential.
- As an *unrelated Linux user*, I want to be unaffected by the operator's scryd: not able to interact with it, not able to read the operator's credential, not weaker against attack because scryd is on the host.

### Acceptance criteria

- Given a Linux host with no scryd installed, when the operator runs the install command with elevation, then within sixty seconds the install command exits zero, the daemon is running in the background, the operator's account-configuration command is on the operator's PATH, and the operator's search command is on the operator's PATH.
- Given an installed scryd with no configured accounts, when the operator runs the configure-account command with elevation and supplies host = `imap.example.com`, port = `993`, user = `alice@example.com`, password = `abcd-efgh-ijkl-mnop`, folders = `INBOX`, then within ten seconds the daemon attempts a fetch from that account using exactly those values and emits one log record naming the account and the attempt outcome.
- Given a configured account with a stored credential, when an adversary process running as the operator attempts to open the file containing the credential by any standard file-system call, then every such attempt returns a permission-denied error and reads zero bytes of the file.
- Given a running daemon, when an adversary process running as the operator attempts to attach a debugger to the daemon process, then the attach attempt returns an operation-not-permitted error and the adversary obtains zero bytes of daemon memory.
- Given an indexed corpus of one operator's mail, when the operator's terminal issues the query `annual invoice from acme`, then the response arrives in under two hundred milliseconds and the top three results contain at least one message in which both the words `invoice` and `acme` appear within the same message body, with each result citing the source folder, sender, and date.
- Given an indexed corpus, when a local agent issues the query `annual invoice from acme`, then it receives the same ranked result list and the same response time as the operator's terminal received in the previous criterion.
- Given a configured account whose credential the upstream provider has rotated, when the operator runs the rotate-password command and supplies the new credential, then the daemon's next sync attempt for that account succeeds using the new credential, the existing index is preserved, no reindex occurs, and the daemon emits one log record naming the rotation.
- Given a configured account, when the operator runs the remove-account command for that account, then within five seconds the daemon stops attempting to sync the account, and within sixty seconds search results no longer return messages from that account.
- Given an installed scryd, when the operator runs the uninstall command, then within thirty seconds every file the install produced is removed, no scryd process continues to run, the daemon's Linux account is removed, and the operator's search command is no longer on the operator's PATH.
- Given a different Linux user on the same host who has not been granted access by the operator or by root, when that user runs the search command, then the command exits non-zero with an error message that names "scryd is not reachable for this user" and does not reveal whether the operator runs scryd.
- Given a different Linux user on the same host, when that user attempts to read the operator's stored credential file, then the read returns a permission-denied error.

### Failure modes

- Given the IMAP server is unreachable, when scryd attempts to sync, then the daemon emits one log record naming the account, the attempt count, and the underlying network error; serves previously-indexed mail to search queries without interruption; and retries on a backoff schedule until the server becomes reachable.
- Given the configured credential is rejected by the IMAP server, when scryd attempts to sync, then the daemon emits one log record naming the account and the rejection; halts further sync attempts for that account until the operator rotates the credential; and continues to serve previously-indexed mail to search queries.
- Given the IMAP server rate-limits scryd, when scryd reaches the limit, then the daemon emits one log record naming the account and the wait duration the server requested; backs off for that duration; and continues to serve previously-indexed mail.
- Given the host's disk is full, when scryd attempts to write a new message or extend the index, then the daemon emits one log record naming the disk-full condition; halts ingestion of new messages from sync; and continues to serve previously-indexed mail.
- Given the daemon process crashes, when the host's service manager notices, then the daemon is restarted within ten seconds, replays any in-progress sync state from durable storage on startup, and accepts CLI and local-agent connections again immediately on restart.
- Given the operator runs the search command and the daemon is not running on the host, when the command attempts to connect, then it exits non-zero with an error message naming "scryd is not running" and a one-line hint for starting it.
- Given the operator runs the configure-account command without elevation, when the command starts, then it exits non-zero before reading any credential input, with an error message naming the command and stating elevation is required.
- Given the operator runs the configure-account command with elevation but the daemon is not running, when the command writes the new account, then the command starts the daemon before exiting and emits one log record naming the start.
- Given an upgrade attempt from v0.1.0 to v0.2.0 on a host that has v0.1.0 installed, when the operator runs the v0.2.0 install command without an explicit confirmation flag, then the install command detects the v0.1.0 layout, exits zero with no changes, and prints a message naming the v0.1.0 → v0.2.0 transition as not data-preserving and naming the confirmation flag the operator must pass to proceed. The v0.1.0 install remains intact.
- Given the same situation, when the operator re-runs the v0.2.0 install command with the confirmation flag, then the install command stops the v0.1.0 daemon, removes the v0.1.0 binaries, configuration, indexed data, and downloaded weights, and proceeds with the v0.2.0 install. The v0.2.0 daemon starts with an empty index; the operator runs the configure-account command afresh; the daemon resyncs from the IMAP server.
- Given the daemon's required model weights are missing or their integrity hash does not match what the daemon expects, when the daemon starts, then the daemon refuses to start, emits one log record naming the missing-weights or hash-mismatch condition, and the install command's status check reports the condition with a one-line hint for re-downloading.

## §3 Scope

### In

- A daemon that runs in the background under a Linux account distinct from the operator's account, with that account owning the IMAP credential at rest and in memory.
- An install command that requires elevation, creates the daemon's Linux account, sets up the daemon's owned directories, downloads the model weights, configures the daemon to start on boot, starts the daemon, and places the operator's CLI commands on the operator's PATH — all in one invocation.
- A CLI for the operator that runs without elevation for read-only operations (search, retrieve a message, retrieve a thread, list accounts, view daemon status) and requires elevation for operations that mutate the daemon's stored configuration (configure-account, rotate-password, remove-account).
- A local search interface, addressable by any program running under the operator's Linux account, with peer-account verification at the daemon side that admits the operator's account and rejects every other account.
- A guarantee that the IMAP credential is absent from any file the operator's account can read, any process memory the operator's account can inspect, any environment variable the operator's account can read, and any output any non-elevated CLI command produces.
- An invariant that the daemon's behavior visible to a local agent is identical to the daemon's behavior visible to the operator's interactive CLI: same query interface, same response shape, same failure modes.
- An uninstall command that removes the daemon's account, the daemon's owned directories, the daemon's startup configuration, the binaries the install placed, and any runtime state the daemon produced.
- A documented upgrade path within the v0.2.x series that preserves operator configuration, the index, and the downloaded weights.

### Out

- Multi-operator support on a single host (more than one Linux user simultaneously sharing one daemon for distinct mail accounts).
- Authentication of local programs beyond the peer-account check (no API tokens, no OAuth or session protocol between operator and daemon).
- OAuth-based or token-based authentication to the IMAP server (v0.2.0 requires an app password; provider-specific token flows are deferred).
- Storage of the IMAP credential outside the daemon's owned files (operating-system keychain, hardware-security-module binding, remote secret store).
- Any operating system other than Linux.
- A graphical or web user interface; v0.2.0 ships a CLI plus the local search interface and nothing more.
- Sending mail. scryd remains read-only against the IMAP server.
- Indexing of attachments' contents. v0.2.0 indexes message metadata and message bodies; attachment file names appear in metadata, attachment contents are not parsed.
- Cross-host operation. One daemon serves one operator's mail on one host.
- A dry-run mode for the uninstall command. The uninstall command performs its work directly; the artifacts it removes are documented and an operator who wants to verify in advance reads the install documentation.
- Automatic migration of the v0.1.0 index into v0.2.0. The version transition removes the v0.1.0 data and starts v0.2.0 with an empty index, on the grounds that v0.1.0 was a preview release.

## §4 Quality bars

- p95 search-end-to-end latency, measured from CLI invocation to first result rendered in the operator's terminal, under two hundred milliseconds for a corpus of ten thousand messages on warm cache.
- p99 search latency, same conditions, under five hundred milliseconds.
- The daemon achieves at least 99.5 percent successful response rate over rolling-24-hour windows on a host with continuous network connectivity to the IMAP server.
- New mail is visible to a search query within sixty seconds of the message arriving on the IMAP server when the server supports IDLE notifications, and within the operator's configured polling interval otherwise.
- A program running under the operator's Linux account does not, by any sequence of standard system calls, recover the stored IMAP credential. The guarantee is judged against the kernel's normal access controls; any path that requires escalating to root or to the daemon's account is out of scope of this guarantee.
- A different Linux user on the same host, without explicit privilege granted by the operator or by root, does not interact with the operator's scryd, read the operator's credential, observe the operator's mail, or determine whether the operator runs scryd.
- The release runs on x86_64 and aarch64, on current Debian, current Ubuntu LTS, current Fedora, and current Arch.

### Trust modes

v0.2.0 ships one trust mode: **single-operator host**. The host has one human operator who consents to the install. The daemon's Linux account is created at install time and is the sole owner of the credential. The operator can grant or revoke a local agent's access to the search interface only by changing the operator's own Linux account itself; there is no per-program access control.

A future trust mode, **multi-operator host**, in which several Linux users on the same host each have their own daemon instance with their own credential and none can read each other's data, is not provisioned by v0.2.0's install command. The architecture admits this configuration via a future installer change, with no runtime-code change. v0.2.0's spec does not constrain that future shape.

> If any requirement is ambiguous, stop and ask. Before producing a plan, list your assumptions and the implementation choices you intend to make. Do not write code until those are confirmed.
