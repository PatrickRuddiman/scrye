//! Integration test: fetch_batch returns parsed FetchedMessages with
//! distinct UIDs and non-empty raw bodies after injecting fixture
//! mail into GreenMail. Skips silently when the fixture isn't
//! reachable.

#![cfg(unix)]

mod support;

use scryd_imap::connect::{login, LoggedIn};
use scryd_imap::fetch::fetch_batch;

#[tokio::test]
async fn fetch_batch_returns_injected_messages() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    // Inject 3 unique-prefixed messages.
    let prefix = support::greenmail::unique_prefix();
    for i in 0..3 {
        support::greenmail::inject_message(
            "alice@localhost",
            "test@localhost",
            &format!("{prefix}-{i}"),
            &format!("body-{prefix}-{i}"),
        )
        .await
        .expect("inject_message");
    }
    // Allow GreenMail to commit before EXAMINE.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let logged_in = login(
        &support::greenmail::host(),
        support::greenmail::imap_port(),
        false,
        "test",
        "test",
        "fetch-test",
    )
    .await
    .expect("login");

    let mut client = match logged_in {
        LoggedIn::Plain(c) => c,
        LoggedIn::Tls(_) => unreachable!("plain login must return Plain"),
    };

    client.examine("INBOX").await.expect("examine INBOX");

    let messages = fetch_batch(&mut client, "1:*", "fetch-test", "INBOX", 0)
        .await
        .expect("fetch_batch");

    let our_messages: Vec<_> = messages
        .iter()
        .filter(|m| {
            std::str::from_utf8(&m.raw_bytes)
                .map(|s| s.contains(&prefix))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        our_messages.len() >= 3,
        "expected >= 3 messages with prefix {prefix}, got {} (total returned: {})",
        our_messages.len(),
        messages.len()
    );

    let mut uids: Vec<u32> = our_messages.iter().map(|m| m.server_uid).collect();
    uids.sort();
    uids.dedup();
    assert_eq!(
        uids.len(),
        our_messages.len(),
        "duplicate server_uids returned"
    );
    for m in &our_messages {
        assert!(
            !m.raw_bytes.is_empty(),
            "raw_bytes empty for uid {}",
            m.server_uid
        );
        let raw = std::str::from_utf8(&m.raw_bytes).expect("utf-8 message");
        assert!(raw.contains("Subject:"), "missing Subject header in raw");
        assert_eq!(m.account_id, "fetch-test");
        assert_eq!(m.folder, "INBOX");
    }
}
