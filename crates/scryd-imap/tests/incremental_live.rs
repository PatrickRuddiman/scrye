//! Integration test: run_incremental fetches only messages with UID
//! greater than the supplied watermark and updates state with the
//! new high-water mark.

#![cfg(unix)]

mod support;

use scryd_imap::connect::{login, LoggedIn};
use scryd_imap::fetch::{run_incremental, run_initial_backfill};
use scryd_imap::state::Connection;

#[tokio::test]
async fn run_incremental_fetches_only_new_uids_and_updates_watermark() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    let prefix = support::greenmail::unique_prefix();

    // 5 messages first.
    for i in 0..5 {
        support::greenmail::inject_message(
            "alice@localhost",
            "test@localhost",
            &format!("{prefix}-A-{i}"),
            "first-batch",
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
        "incremental-test",
        None,
    )
    .await
    .expect("login");
    let mut client = match logged_in {
        LoggedIn::Plain(c) => c,
        LoggedIn::Tls(_) => unreachable!(),
    };

    let sink = support::test_sink::CollectingSink::new();
    let mut conn = Connection::new("incremental-test", "INBOX");

    run_initial_backfill(&mut conn, &mut client, &sink)
        .await
        .expect("backfill");

    let after_backfill = sink.submitted_count();
    let watermark = sink
        .state_updates
        .lock()
        .unwrap()
        .last()
        .and_then(|(_, _, u)| u.last_seen_uid)
        .expect("backfill recorded a last_seen_uid");

    // 3 more messages now.
    for i in 0..3 {
        support::greenmail::inject_message(
            "alice@localhost",
            "test@localhost",
            &format!("{prefix}-B-{i}"),
            "second-batch",
        )
        .await
        .expect("inject_message");
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let new_watermark = run_incremental(&mut conn, &mut client, &sink, watermark)
        .await
        .expect("incremental");

    assert!(
        new_watermark > watermark,
        "watermark must advance: {watermark} -> {new_watermark}"
    );

    let new_messages = sink.submitted_count() - after_backfill;
    // Server may include messages from earlier inject runs that share
    // the same INBOX. We only require that at least the 3 new messages
    // we injected after the watermark show up.
    assert!(
        new_messages >= 3,
        "expected >= 3 new messages after watermark, got {new_messages}"
    );

    // No duplicates by server_uid in the full submitted list.
    let submitted = sink.submitted.lock().unwrap();
    let mut uids: Vec<u32> = submitted.iter().map(|m| m.server_uid).collect();
    uids.sort();
    let total = uids.len();
    uids.dedup();
    assert_eq!(uids.len(), total, "duplicate server_uid in submitted set");
}
