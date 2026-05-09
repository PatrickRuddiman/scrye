Parent plan: GreenMail integration testing for scryd
Depends on: none

# Task 00 — greenmail-fixture-support

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land a small test-support module that connects to a running GreenMail container, injects messages via SMTP (lettre), clears server state via the REST API, and skips with a clear message when the fixture isn't reachable. Subsequent tasks (02–07) consume this module.

## Tasks
- [x] Add `lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1", "builder"] }` to `crates/scryd-imap/Cargo.toml` `[dev-dependencies]`. (No TLS feature: GreenMail accepts plain SMTP in test mode and `builder_dangerous` skips TLS.)
- [x] Create `crates/scryd-imap/tests/support/mod.rs` re-exporting the GreenMail helpers.
- [x] Create `crates/scryd-imap/tests/support/greenmail.rs` with:
  - `pub fn host() -> String` (default `127.0.0.1`, override via `SCRYD_TEST_GREENMAIL_HOST`).
  - `pub fn imap_port() / smtp_port() -> u16` (defaults 3143 / 3025).
  - `pub fn skip_if_unreachable() -> bool` using a synchronous `std::net::TcpStream::connect_timeout(_, 1s)` so it's safe to call from inside a `#[tokio::test]` runtime.
  - `pub async fn inject_message(from, to, subject, body)` via `lettre::AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host).port(smtp_port).build()`.
  - `pub async fn inject_n(n, sender, recipient)` firing `test-{i}` subjects.
  - `pub fn unique_prefix() -> String` returning a per-run subject prefix (epoch-nanos hex). GreenMail standalone has no usable programmatic reset; tests use unique subjects for state isolation across runs.
- [x] Document the local-dev quickstart at the top of `tests/support/greenmail.rs` as a doc-comment.
- [x] Add `crates/scryd-imap/tests/greenmail_smoke.rs` with one test: `inject_one_message_round_trips_or_skips`. Calls `support::skip_if_unreachable()`; on `true` returns Ok silently. On `false` injects one message via `inject_message` and asserts Ok.
- [x] Decision recorded: hand-rolled HTTP not added since GreenMail standalone v2.x doesn't expose a usable REST reset endpoint by default. Tests rely on `unique_prefix()` for state isolation instead.

## Acceptance criteria
- [x] `cargo build -p scryd-imap --tests` exits 0.
- [x] `cargo test -p scryd-imap --test greenmail_smoke` passes (1/1 against running GreenMail; passes silently otherwise).
- [x] `git grep -F 'greenmail/standalone' crates/scryd-imap/tests/support/greenmail.rs` matches the docker quickstart.
- [x] `git grep -nE 'pub fn skip_if_unreachable|pub async fn inject_message|pub fn unique_prefix' crates/scryd-imap/tests/support/greenmail.rs | wc -l` returns at least 3.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
