Parent slice: [imap-sync](../slices/imap-sync.md)
Depends on: 00, 01, 02

# Task 12 — scryd-imap-client

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Wrap `async-imap` behind a thin client that is structurally read-only: an enum over the entire allow-list of verbs, a wrapper that refuses any verb outside it, EXAMINE-only mailbox opens, and a CAPABILITY check that probes IDLE.

## Tasks
- [x] In `crates/scryd-imap/Cargo.toml`, add deps: `async-imap`, `tokio-rustls`, `rustls`, `webpki-roots`, `tokio` (with `net`, `io-util`, `time`, `sync`, `rt`, `macros`), `scryd-config` (path = `../scryd-config`), `scryd-log` (path = `../scryd-log`), `serde`, `thiserror`, `tracing`, `bytes`.
- [x] In `crates/scryd-imap/src/verbs.rs`, define `pub enum ImapVerb { Capability, Login, Authenticate, Logout, Examine, List, Lsub, Status, Fetch, UidFetch, Search, UidSearch, Idle, Done, Noop, Id, Enable, GetMetadata, GetAcl, GetQuota, GetQuotaRoot }`. Implement `pub fn assert_verb_allowed(v: &ImapVerb)` that panics in `debug_assertions` and returns `Err(ClientError::MutatingCommandRejected)` in release for any future verb addition that's not in the enum (the enum is exhaustive by construction; this function exists so `clippy` and review can lint for unsafe paths).
- [x] In `crates/scryd-imap/src/tls.rs`, expose `pub async fn connect_tls(host: &str, port: u16) -> Result<TlsStream, ClientError>` using `tokio-rustls` with `webpki-roots` trust anchors. No per-account `skip-verify` option.
- [x] In `crates/scryd-imap/src/client.rs`, expose `pub struct Client` wrapping the `async-imap::Session<TlsStream>` after authentication, with helper methods `examine(folder: &str)`, `capability()`, `uid_fetch(range, items)`, `uid_search(criteria)`, `idle_start()`, `idle_done()`, `noop()`, `logout()`. Each helper invokes the underlying `async-imap` session; each helper internally calls `assert_verb_allowed(ImapVerb::…)` first.
- [x] In `crates/scryd-imap/src/connect.rs`, expose `pub async fn login(host: &str, port: u16, user: &str, password: &str) -> Result<Client, ClientError>`. Order: `connect_tls` → `async-imap::Client::new` → `login(user, password)` → `Client { session }`. Map errors to `ClientError::AuthRejected`, `ClientError::TlsHandshake`, `ClientError::Connect`, `ClientError::Server` and emit the corresponding `log_failure!` lines using categories `auth rejection`, `tls failure`, `connect failure`.
- [x] In `crates/scryd-imap/src/capabilities.rs`, expose `pub struct Capabilities { idle: bool, … }` and `Client::capabilities(&self) -> Result<Capabilities, ClientError>` that runs CAPABILITY and parses out `IDLE` (and any other extension flags we use later).
- [x] Write unit tests in `crates/scryd-imap/tests/verbs.rs`: assert `ImapVerb` is `#[non_exhaustive]` is NOT used (we want exhaustive matching at call sites); assert `assert_verb_allowed` returns `Ok` for each variant.
- [ ] Write integration tests in `crates/scryd-imap/tests/client.rs` against a minimal mock IMAP server (use the `imap-server-mock` crate or hand-roll one in `tests/support/mock_server.rs`): connect, examine `INBOX`, run `capabilities`, assert `idle = true` when the mock advertises IDLE; assert `LOGIN` failure surfaces as `ClientError::AuthRejected` and emits one `auth rejection` log line. **Deferred** — hand-rolled async IMAP mock server is its own task; the verb allowlist + capabilities parsing + connect-flow error mapping is unit-tested.

## Acceptance criteria
- [ ] `cargo test -p scryd-imap --test verbs --test client` passes. **Partial**: `--test verbs` and `--test capabilities` pass; `--test client` deferred with the integration test.
- [x] `cargo check -p scryd-imap` exits 0.
- [x] `git grep -nE 'enum ImapVerb' crates/scryd-imap/src/verbs.rs` matches and the variants list contains exactly the 21 names listed in the slice §4 enum.
- [x] `git grep -cE 'assert_verb_allowed' crates/scryd-imap/src/client.rs` returns at least 8 matches (one per helper method).
- [x] `git grep -nE 'category::AUTH_REJECTION|category::TLS_FAILURE|category::CONNECT_FAILURE' crates/scryd-imap/src/connect.rs` matches all three.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
