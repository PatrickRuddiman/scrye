//! Helpers that talk to a running GreenMail container. Tests call
//! `skip_if_unreachable()` first; on `true` the test prints a hint
//! and returns Ok so CI / local runs without the fixture pass
//! silently.
//!
//! Local-dev quickstart:
//!
//! ```text
//! docker run -d --rm --name greenmail \
//!   -p 3025:3025 -p 3143:3143 -p 8080:8080 \
//!   -e GREENMAIL_OPTS="-Dgreenmail.setup.test.smtp -Dgreenmail.setup.test.imap \
//!     -Dgreenmail.smtp.hostname=0.0.0.0 -Dgreenmail.imap.hostname=0.0.0.0 \
//!     -Dgreenmail.users=test:test@localhost -Dgreenmail.auth.disabled" \
//!   greenmail/standalone:latest
//! ```

use std::time::Duration;

use lettre::{
    message::Message, transport::smtp::AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};

pub fn host() -> String {
    std::env::var("SCRYD_TEST_GREENMAIL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
}

pub fn imap_port() -> u16 {
    std::env::var("SCRYD_TEST_GREENMAIL_IMAP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3143)
}

pub fn smtp_port() -> u16 {
    std::env::var("SCRYD_TEST_GREENMAIL_SMTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3025)
}

const QUICKSTART_HINT: &str = "set SCRYD_TEST_GREENMAIL_HOST or run:\n  \
    docker run -d --rm --name greenmail -p 3025:3025 -p 3143:3143 \\\n    \
      -e GREENMAIL_OPTS=\"-Dgreenmail.smtp.hostname=0.0.0.0 -Dgreenmail.smtp.port=3025 \
    -Dgreenmail.imap.hostname=0.0.0.0 -Dgreenmail.imap.port=3143 \
    -Dgreenmail.users=test:test@localhost -Dgreenmail.auth.disabled\" \\\n    \
      greenmail/standalone:latest";

/// Probe IMAP port; return `true` if the test should skip. Uses a
/// synchronous std TcpStream so it's safe to call from inside a
/// `#[tokio::test]` runtime.
pub fn skip_if_unreachable() -> bool {
    use std::net::ToSocketAddrs;
    let h = host();
    let p = imap_port();
    let connectable = (h.as_str(), p)
        .to_socket_addrs()
        .ok()
        .and_then(|mut iter| iter.next())
        .and_then(|addr| {
            std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(1)).ok()
        })
        .is_some();
    if !connectable {
        eprintln!(
            "scryd-imap test fixture not reachable at {}:{}; skipping. {}",
            h, p, QUICKSTART_HINT
        );
    }
    !connectable
}

/// Inject a single message into GreenMail's SMTP. Returns Ok on
/// success; the caller can `?` it.
pub async fn inject_message(
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let msg = Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject(subject)
        .body(body.to_string())?;

    let transport: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host())
            .port(smtp_port())
            .build();

    transport.send(msg).await?;
    Ok(())
}

/// Inject `n` messages with subjects `test-{i}` from `sender@localhost`
/// to `recipient`. Useful for backfill / fetch tests that grep for a
/// known subject substring.
pub async fn inject_n(
    n: usize,
    sender: &str,
    recipient: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for i in 0..n {
        let subject = format!("test-{i}");
        let body = format!("body-{i}");
        inject_message(sender, recipient, &subject, &body).await?;
    }
    Ok(())
}

/// Generate a per-run subject prefix tests can use to isolate their
/// fixture mail from other tests' (and from prior runs against the
/// same long-lived container). GreenMail's standalone image does not
/// expose a programmatic reset, so subject-based filtering is the
/// pragmatic isolation primitive.
pub fn unique_prefix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("scrydtest-{nanos:x}")
}
