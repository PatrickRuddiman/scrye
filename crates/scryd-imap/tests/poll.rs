use scryd_imap::poll::{effective_poll_interval, POLL_INTERVAL_FLOOR};

#[test]
fn floor_clamps_tiny_intervals() {
    assert_eq!(effective_poll_interval(0), POLL_INTERVAL_FLOOR);
    assert_eq!(effective_poll_interval(5), POLL_INTERVAL_FLOOR);
    assert_eq!(effective_poll_interval(29), POLL_INTERVAL_FLOOR);
}

#[test]
fn floor_passes_through_normal_intervals() {
    assert_eq!(
        effective_poll_interval(300),
        std::time::Duration::from_secs(300)
    );
    assert_eq!(
        effective_poll_interval(60),
        std::time::Duration::from_secs(60)
    );
}

#[test]
fn floor_is_30_seconds() {
    assert_eq!(POLL_INTERVAL_FLOOR, std::time::Duration::from_secs(30));
}
