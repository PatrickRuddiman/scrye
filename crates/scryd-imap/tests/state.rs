use scryd_imap::{ConnState, Connection, INCREMENTAL_FETCH_BATCH};

#[test]
fn fresh_connection_starts_disconnected() {
    let c = Connection::new("primary", "INBOX");
    assert_eq!(c.account_id, "primary");
    assert_eq!(c.folder, "INBOX");
    assert_eq!(c.state, ConnState::Disconnected);
    assert!(!c.is_connected());
}

#[test]
fn enter_backfill_resets_watermark() {
    let mut c = Connection::new("primary", "INBOX");
    c.enter_backfill(Some(1000));
    match c.state {
        ConnState::InitialBackfilling { last_uid, target } => {
            assert_eq!(last_uid, 0);
            assert_eq!(target, Some(1000));
        }
        other => panic!("expected InitialBackfilling, got {other:?}"),
    }
    assert!(c.is_connected());
}

#[test]
fn advance_backfill_only_moves_forward() {
    let mut c = Connection::new("primary", "INBOX");
    c.enter_backfill(None);
    c.advance_backfill(500);
    if let ConnState::InitialBackfilling { last_uid, .. } = c.state.clone() {
        assert_eq!(last_uid, 500);
    } else {
        panic!("not backfilling");
    }
    // Going backwards is rejected.
    c.advance_backfill(100);
    if let ConnState::InitialBackfilling { last_uid, .. } = c.state.clone() {
        assert_eq!(last_uid, 500);
    } else {
        panic!("not backfilling");
    }
    c.advance_backfill(750);
    if let ConnState::InitialBackfilling { last_uid, .. } = c.state {
        assert_eq!(last_uid, 750);
    } else {
        panic!("not backfilling");
    }
}

#[test]
fn advance_backfill_outside_backfill_state_is_a_noop() {
    let mut c = Connection::new("primary", "INBOX");
    c.advance_backfill(100);
    assert_eq!(c.state, ConnState::Disconnected);
}

#[test]
fn enter_backoff_records_until_timestamp() {
    let mut c = Connection::new("primary", "INBOX");
    c.enter_backoff(1_700_000_000);
    assert_eq!(
        c.state,
        ConnState::Backoff {
            until_unix_secs: 1_700_000_000
        }
    );
    assert!(!c.is_connected());
}

#[test]
fn is_connected_only_true_for_active_states() {
    let mut c = Connection::new("primary", "INBOX");
    let cases = [
        (ConnState::Disconnected, false),
        (ConnState::Resolving, false),
        (ConnState::Connecting, false),
        (ConnState::TlsHandshaking, false),
        (ConnState::LoggingIn, false),
        (ConnState::CapabilityChecking, true),
        (ConnState::Selecting, true),
        (
            ConnState::InitialBackfilling {
                last_uid: 0,
                target: None,
            },
            true,
        ),
        (ConnState::Idling, true),
        (ConnState::Polling, true),
        (ConnState::Fetching, true),
        (ConnState::Backoff { until_unix_secs: 0 }, false),
    ];
    for (state, expected) in cases {
        c.state = state.clone();
        assert_eq!(
            c.is_connected(),
            expected,
            "is_connected() mismatch for {state:?}"
        );
    }
}

#[test]
fn fetch_batch_constant_is_500() {
    assert_eq!(INCREMENTAL_FETCH_BATCH, 500);
}
