Parent plan: GreenMail integration testing for scryd
Depends on: none

# Task 01 — accountcfg-tls-bool

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Add a `tls: bool` field to `AccountCfg` (defaults `true`) and route the connect path through a plain-TCP login when `tls = false`. Production stays on TLS; tests opt into plain-IMAP for the GreenMail fixture. This is **TLS-or-not**, not cert-skip-verify — the spec's prohibition on skip-verify is preserved.

## Tasks
- [ ] In `crates/scryd-config/src/loader.rs:80-88` (`AccountCfg`), add `#[serde(default = "default_tls")] pub tls: bool` and a `fn default_tls() -> bool { true }`. Document with a doc-comment that this defaults to `true` and `false` is for test fixtures only.
- [ ] Update existing tests in `crates/scryd-config/tests/loader.rs` that consume `AccountCfg` to either rely on the default or set `tls = true` explicitly where they assert field equality.
- [ ] Create `crates/scryd-imap/src/plain.rs`. Expose `pub type PlainStream = Compat<TcpStream>` and `pub async fn connect_plain(host: &str, port: u16) -> Result<PlainStream, ClientError>` mirroring the shape of `connect_tls` from `tls.rs:9-37` but skipping the TLS handshake. `tokio_util::compat::TokioAsyncReadCompatExt::compat()` is reused.
- [ ] In `crates/scryd-imap/src/lib.rs`, expose `pub mod plain;`.
- [ ] In `crates/scryd-imap/src/connect.rs`, refactor:
  - Rename the existing `login` to `login_tls`.
  - Add a new `pub async fn login_plain(host, port, user, password, account_id) -> Result<Client<PlainStream>, ClientError>` that mirrors `login_tls` but calls `connect_plain` instead.
  - Keep both functions returning `Client<S>` over their respective `S`.
- [ ] Add an enum-shaped helper `pub enum LoggedIn { Tls(Client<TlsStream>), Plain(Client<PlainStream>) }` plus `pub async fn login(host, port, tls: bool, user, password, account_id) -> Result<LoggedIn, ClientError>` that dispatches. The supervisor (task 07) will use `login(.., account.tls, ..)`.
- [ ] Update `crates/scryd-imap/src/credential.rs` (if it consumes Client<S>) — should already be generic.
- [ ] Add `crates/scryd-imap/tests/plain_connect.rs`: a `cfg(unix)` integration test that uses the support module's `skip_if_unreachable`, then calls `connect::login(.., tls = false, ..)` against the local GreenMail and asserts the result is `LoggedIn::Plain(_)`. Skip silently if GreenMail unreachable.

## Acceptance criteria
- [ ] `cargo test -p scryd-config --test loader` passes (existing 14 tests + any added).
- [ ] `cargo build -p scryd-imap` exits 0.
- [ ] `cargo test -p scryd-imap --test plain_connect` passes locally with GreenMail running (skips silently otherwise).
- [ ] `git grep -nE 'pub tls: bool' crates/scryd-config/src/loader.rs` matches.
- [ ] `git grep -nE 'fn default_tls' crates/scryd-config/src/loader.rs` matches.
- [ ] `git grep -nE 'pub async fn login_plain|pub async fn login_tls|pub enum LoggedIn' crates/scryd-imap/src/connect.rs | wc -l` returns at least 3.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
