//! Integration test: run_idle_loop wakes on EXISTS pushed by GreenMail
//! after a fresh inject, runs run_incremental, and submits the new
//! message to the sink. Shutdown via the watch channel cleanly returns.

#![cfg(unix)]

mod support;

use std::time::Duration;

use scryd_imap::connect::{login, LoggedIn};
use scryd_imap::fetch::run_initial_backfill;
use scryd_imap::idle::run_idle_loop;
use scryd_imap::state::Connection;

#[tokio::test]
async fn run_idle_loop_picks_up_new_inject_within_budget() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    let prefix = support::greenmail::unique_prefix();
    // Seed mailbox so backfill has something to do.
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
        "idle-test",
        None,
    )
    .await
    .expect("login");
    let mut client = match logged_in {
        LoggedIn::Plain(c) => c,
        LoggedIn::Tls(_) => unreachable!(),
    };

    let sink = std::sync::Arc::new(support::test_sink::CollectingSink::new());
    let mut conn = Connection::new("idle-test", "INBOX");

    let (_backfill_shutdown_tx, backfill_shutdown_rx) = tokio::sync::watch::channel(false);
    run_initial_backfill(&mut conn, &mut client, sink.as_ref(), backfill_shutdown_rx)
        .await
        .expect("backfill");
    let watermark = sink
        .state_updates
        .lock()
        .unwrap()
        .last()
        .and_then(|(_, _, u)| u.last_seen_uid)
        .expect("backfill last_seen_uid");
    let pre_count = sink.submitted_count();

    let (tx, rx) = tokio::sync::watch::channel(false);
    let sink_clone = sink.clone();
    let mut conn_clone = conn.clone();

    let handle = tokio::spawn(async move {
        run_idle_loop(
            &mut conn_clone,
            client,
            sink_clone.as_ref(),
            watermark,
            Duration::from_millis(500),
            rx,
        )
        .await
    });

    // Give the IDLE loop a moment to enter IDLE.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Inject a new message; GreenMail pushes EXISTS over the IDLE channel.
    support::greenmail::inject_message(
        "alice@localhost",
        "test@localhost",
        &format!("{prefix}-B-new"),
        "after-idle",
    )
    .await
    .expect("inject new");

    // Wait up to 5s for the loop to react.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if sink.submitted_count() > pre_count {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            tx.send(true).ok();
            let _ = handle.await;
            panic!(
                "IDLE loop did not pick up the new message within 5s; submitted_count stuck at {pre_count}"
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Shutdown cleanly.
    tx.send(true).expect("send shutdown");
    let result = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("loop should return on shutdown")
        .expect("task did not panic")
        .expect("loop returned Ok");

    let (_returned_client, returned_uid) = result;
    assert!(
        returned_uid >= watermark,
        "returned watermark must not regress"
    );
}
