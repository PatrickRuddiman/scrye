//! Integration test: run_initial_backfill walks INBOX in
//! INCREMENTAL_FETCH_BATCH-sized batches, hands every message to the
//! sink, and updates sync_state with the high-water UID.

#![cfg(unix)]

mod support;

use scryd_imap::connect::{login, LoggedIn};
use scryd_imap::fetch::run_initial_backfill;
use scryd_imap::state::Connection;

#[tokio::test]
async fn run_initial_backfill_submits_each_message_and_updates_state() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    let prefix = support::greenmail::unique_prefix();
    let n = 7;
    for i in 0..n {
        support::greenmail::inject_message(
            "alice@localhost",
            "test@localhost",
            &format!("{prefix}-{i}"),
            &format!("body-{prefix}-{i}"),
        )
        .await
        .expect("inject_message");
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let logged_in = login(
        &support::greenmail::host(),
        support::greenmail::imap_port(),
        false,
        "test",
        "test",
        "backfill-test",
        None,
    )
    .await
    .expect("login");

    let mut client = match logged_in {
        LoggedIn::Plain(c) => c,
        LoggedIn::Tls(_) => unreachable!("plain login must return Plain"),
    };

    let sink = support::test_sink::CollectingSink::new();
    let mut conn = Connection::new("backfill-test", "INBOX");

    run_initial_backfill(&mut conn, &mut client, &sink)
        .await
        .expect("run_initial_backfill");

    let submitted = sink.submitted.lock().unwrap();
    let our_messages: Vec<_> = submitted
        .iter()
        .filter(|m| {
            std::str::from_utf8(&m.raw_bytes)
                .map(|s| s.contains(&prefix))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        our_messages.len() >= n,
        "expected {n} submitted matches for {prefix}, got {}",
        our_messages.len()
    );

    // The sink must see at least one update_sync_state call carrying
    // a uidvalidity and a last_seen_uid.
    let updates = sink.state_updates.lock().unwrap();
    assert!(!updates.is_empty(), "no state updates recorded");
    let last = updates.last().unwrap();
    assert_eq!(last.0, "backfill-test");
    assert_eq!(last.1, "INBOX");
    assert!(last.2.uidvalidity.is_some());
    assert!(last.2.last_seen_uid.is_some());
}
