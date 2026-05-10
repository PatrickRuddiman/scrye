Parent plan: scryd v0.3.1 — service pivot
Depends on: none

# Task 11 — tls-custom-ca-path

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Honor the per-account `tls_ca_path` field from task 00. When set, the TLS connect path uses that PEM file as the trust anchor instead of the baked-in `webpki-roots`. Self-signed corporate IMAP servers work without rebuilding scryd against a custom rustls trust store. The spec's prohibition on cert-skip-verify still holds — this is honest TLS with a caller-supplied CA.

## Tasks
- [ ] In `crates/scryd-imap/src/tls.rs`, add `pub async fn connect_tls_with_ca(host: &str, port: u16, ca_path: &Path) -> Result<TlsStream, ClientError>` that:
  - Reads the PEM file via `std::fs::read`.
  - Parses one or more certificates via `rustls_pemfile::certs(...)`.
  - Builds a `RootCertStore` from those certs (no `webpki-roots` mixed in).
  - Otherwise mirrors `connect_tls`'s flow.
- [ ] Refactor `connect_tls` and `connect_tls_with_ca` to share the post-handshake plumbing via a private `build_connector(roots: RootCertStore) -> TlsConnector` helper.
- [ ] In `crates/scryd-imap/src/connect.rs:14-62` (`login_tls`), add a `ca_path: Option<&Path>` parameter; route to `connect_tls_with_ca` when `Some`, else `connect_tls`.
- [ ] In `crates/scryd-imap/src/connect.rs:107-122` (the `login` dispatcher), thread the `ca_path` through.
- [ ] In `crates/scryd-imap/src/scheduler.rs::run_one_cycle` (callsite of `login(...)`), pass `account.tls_ca_path.as_deref()` from the borrowed `Config` snapshot.
- [ ] Add `crates/scryd-imap/tests/tls_custom_ca.rs` with two `#[cfg(unix)]` integration tests:
  - `connect_tls_with_ca_accepts_known_ca`: spawn a tiny tokio TCP server on a random port that does a TLS handshake with a self-signed cert generated via `rcgen` (add `rcgen = "0.13"` to `[dev-dependencies]`); call `connect_tls_with_ca(host, port, ca_pem_path)` against the cert's public chain; assert success.
  - `connect_tls_with_ca_rejects_untrusted_cert`: same fixture but call `connect_tls` (no CA path); assert `ClientError::TlsHandshake`.
- [ ] In `tasks/v0.3.1/triage/`, leave a `tls-host-pinning.md` stub that flags "v0.3.5 follow-up: pin certificate fingerprints rather than CA chains for hostile-network deploys".

## Acceptance criteria
- [ ] `cargo test -p scryd-imap --test tls_custom_ca` passes.
- [ ] `git grep -nE 'pub async fn connect_tls_with_ca' crates/scryd-imap/src/tls.rs` matches.
- [ ] `git grep -nE 'ca_path' crates/scryd-imap/src/connect.rs | wc -l` returns at least 3 (login_tls + login + a doc-comment).
- [ ] `cargo build -p scryd-imap` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
