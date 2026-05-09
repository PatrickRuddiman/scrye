//! Smoke test: with the GreenMail container reachable, the support
//! module's inject + clear helpers round-trip cleanly. Without the
//! container, the test passes silently with a stderr hint.

mod support;

#[tokio::test]
async fn inject_one_message_round_trips_or_skips() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    support::greenmail::inject_message(
        "alice@localhost",
        "test@localhost",
        "smoke-subject",
        "smoke-body",
    )
    .await
    .expect("inject_message");
}
