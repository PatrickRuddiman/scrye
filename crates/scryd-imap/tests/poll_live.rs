//! Integration test: run_poll_loop with a sub-floor interval ticks
//! fast enough that an injected message is picked up within the test
//! budget. Shutdown via the watch channel cleanly returns.

#![cfg(unix)]

mod support;

use std::time::Duration;

use scryd_imap::connect::{login, LoggedIn};
use scryd_imap::fetch::run_initial_backfill;
use scryd_imap::poll::run_poll_loop;
use scryd_imap::state::Connection;

#[tokio::test]
async fn run_poll_loop_picks_up_inject_within_budget() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    let prefix = support::greenmail::unique_prefix();
    for i in 0..3 {
        support::greenmail::inject_message(
            "alice@localhost",
            "test@localhost",
            &format!("{prefix}-A-{i}"),
            "seed",
        )
        .await
        .expect("inject");
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    let logged_in = login(
        &support::greenmail::host(),
        support::greenmail::imap_port(),
        false,
        "test",
        "test",
        "poll-test",
        None,
    )
    .await
    .expect("login");
    let mut client = match logged_in {
        LoggedIn::Plain(c) => c,
        LoggedIn::Tls(_) => unreachable!(),
    };

    let sink = std::sync::Arc::new(support::test_sink::CollectingSink::new());
    let mut conn = Connection::new("poll-test", "INBOX");

    run_initial_backfill(&mut conn, &mut client, sink.as_ref())
        .await
        .expect("backfill");
    let watermark = sink
        .state_updates
        .lock()
        .unwrap()
        .last()
        .and_then(|(_, _, u)| u.last_seen_uid)
        .expect("backfill watermark");
    let pre_count = sink.submitted_count();

    let (tx, rx) = tokio::sync::watch::channel(false);
    let sink_clone = sink.clone();
    let mut conn_clone = conn.clone();

    let handle = tokio::spawn(async move {
        run_poll_loop(
            &mut conn_clone,
            client,
            sink_clone.as_ref(),
            watermark,
            Duration::from_millis(150),
            rx,
        )
        .await
    });

    // Inject mid-poll.
    tokio::time::sleep(Duration::from_millis(100)).await;
    support::greenmail::inject_message(
        "alice@localhost",
        "test@localhost",
        &format!("{prefix}-B-new"),
        "after-poll-start",
    )
    .await
    .expect("inject new");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if sink.submitted_count() > pre_count {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            tx.send(true).ok();
            let _ = handle.await;
            panic!("poll loop did not pick up inject within 3s");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tx.send(true).expect("send shutdown");
    let result = tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .expect("loop returns")
        .expect("no panic")
        .expect("loop returned Ok");
    let (_client, returned_uid) = result;
    assert!(returned_uid >= watermark);
}
