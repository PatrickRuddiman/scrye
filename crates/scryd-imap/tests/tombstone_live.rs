//! Integration test: tombstone::scan diffs UID SEARCH ALL against a
//! caller-supplied local UID list and pipes the diff through
//! sink.tombstone. We synthesize the local list with a few UIDs the
//! server doesn't have, since GreenMail standalone has no clean way
//! to issue an EXPUNGE without putting scryd-imap into write mode.

#![cfg(unix)]

mod support;

use scryd_imap::connect::{login, LoggedIn};
use scryd_imap::state::Connection;
use scryd_imap::tombstone::scan;

#[tokio::test]
async fn tombstone_scan_emits_tombstones_for_locally_present_server_absent_uids() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    let prefix = support::greenmail::unique_prefix();
    for i in 0..3 {
        support::greenmail::inject_message(
            "alice@localhost",
            "test@localhost",
            &format!("{prefix}-{i}"),
            "tombstone-fixture",
        )
        .await
        .expect("inject");
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let logged_in = login(
        &support::greenmail::host(),
        support::greenmail::imap_port(),
        false,
        "test",
        "test",
        "tombstone-test",
    )
    .await
    .expect("login");
    let mut client = match logged_in {
        LoggedIn::Plain(c) => c,
        LoggedIn::Tls(_) => unreachable!(),
    };
    client.examine("INBOX").await.expect("examine");

    // What does the server actually have?
    let server_uids = client.uid_search("ALL").await.expect("uid_search");
    assert!(!server_uids.is_empty());

    // Construct a local UID list with two UIDs the server does NOT
    // have alongside one it does.
    let max_server_uid = *server_uids.iter().max().unwrap();
    let phantom_a = max_server_uid + 100_000;
    let phantom_b = max_server_uid + 200_000;
    let mut local_uids: Vec<u32> = vec![server_uids[0], phantom_a, phantom_b];
    local_uids.sort();

    let sink = support::test_sink::CollectingSink::new();
    let conn = Connection::new("tombstone-test", "INBOX");

    let count = scan(&conn, &mut client, &sink, &local_uids)
        .await
        .expect("scan");

    assert_eq!(count, 2, "expected exactly 2 tombstoned uids");
    let tombstoned = sink.tombstoned.lock().unwrap();
    assert_eq!(tombstoned.len(), 2);
    assert!(tombstoned.iter().any(|s| s.ends_with(&format!("/{phantom_a}"))));
    assert!(tombstoned.iter().any(|s| s.ends_with(&format!("/{phantom_b}"))));
}
