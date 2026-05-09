Parent plan: GreenMail integration testing for scryd
Depends on: none

# Task 00 — greenmail-fixture-support

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land a small test-support module that connects to a running GreenMail container, injects messages via SMTP (lettre), clears server state via the REST API, and skips with a clear message when the fixture isn't reachable. Subsequent tasks (02–07) consume this module.

## Tasks
- [ ] Add `lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1", "tokio1-rustls-tls", "builder"] }` to `crates/scryd-imap/Cargo.toml` `[dev-dependencies]`. (rustls-tls feature for STARTTLS support; GreenMail accepts plain or STARTTLS — we use plain.)
- [ ] Create `crates/scryd-imap/tests/support/mod.rs` re-exporting the GreenMail helpers.
- [ ] Create `crates/scryd-imap/tests/support/greenmail.rs` with:
  - `pub const HOST: &str` reading `SCRYD_TEST_GREENMAIL_HOST` (default `127.0.0.1`).
  - `pub const IMAP_PORT: u16 = 3143;` (read from `SCRYD_TEST_GREENMAIL_IMAP_PORT` if set).
  - `pub const SMTP_PORT: u16 = 3025;` (read from `SCRYD_TEST_GREENMAIL_SMTP_PORT` if set).
  - `pub const REST_PORT: u16 = 8080;`.
  - `pub fn skip_if_unreachable() -> bool` that tries `TcpStream::connect((HOST, IMAP_PORT))` with a 1s timeout. Returns `true` (skip) on failure; logs the docker quickstart command.
  - `pub async fn inject_message(from: &str, to: &str, subject: &str, body: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>` building a `lettre::Message` and shipping via `lettre::AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(HOST).port(SMTP_PORT).build()` — no auth, GreenMail accepts everything when `greenmail.auth.disabled=true`.
  - `pub async fn inject_n(n: usize, recipient: &str)` helper that fires N messages with subject `test-{i}` for grep'able test assertions.
  - `pub async fn clear_mailbox(user: &str)` calling GreenMail's REST API `POST http://{HOST}:{REST_PORT}/api/service/reset` (clears all state) — uses `reqwest` (already a workspace dep) or hand-rolled HTTP via tokio TcpStream + raw bytes (avoid pulling reqwest into scryd-imap test deps if it isn't already there).
- [ ] Document the local-dev quickstart at the top of `tests/support/greenmail.rs` as a doc-comment:
  ```text
  docker run -d --rm --name greenmail \
    -p 3025:3025 -p 3143:3143 -p 8080:8080 \
    -e GREENMAIL_OPTS="-Dgreenmail.users=test:test@localhost -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.auth.disabled=true" \
    greenmail/standalone:latest
  ```
- [ ] Add `crates/scryd-imap/tests/greenmail_smoke.rs` (consumes the support module) with one test: `inject_one_message_and_skip_when_fixture_absent`. The test calls `support::skip_if_unreachable()`; on `true` it `eprintln!`s and returns Ok (passes silently). On `false` it injects one message via `inject_message` and asserts the SMTP transport returned Ok.
- [ ] Decide: HTTP client for the REST clear. Either (a) add `reqwest = { workspace = true }` to dev-deps, or (b) hand-roll a 20-line POST via tokio TcpStream. Prefer (b) since the request is fixed-shape — keeps the dev-dep tree small.

## Acceptance criteria
- [ ] `cargo build -p scryd-imap --tests` exits 0.
- [ ] `cargo test -p scryd-imap --test greenmail_smoke` passes when GreenMail is running locally; passes (with stderr message) when not.
- [ ] `git grep -F 'greenmail/standalone' crates/scryd-imap/tests/support/greenmail.rs` matches the docker quickstart.
- [ ] `git grep -nE 'pub fn skip_if_unreachable|pub async fn inject_message|pub async fn clear_mailbox' crates/scryd-imap/tests/support/greenmail.rs | wc -l` returns at least 3.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
