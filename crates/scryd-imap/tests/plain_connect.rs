//! Integration test: connect to a running GreenMail container over
//! plain TCP (no TLS) and LOGIN as the fixture user. Skips silently
//! when the fixture isn't reachable.

#![cfg(unix)]

mod support;

use scryd_imap::connect::{login, LoggedIn};

#[tokio::test]
async fn plain_login_to_greenmail_returns_loggedin_plain() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    let result = login(
        &support::greenmail::host(),
        support::greenmail::imap_port(),
        false,
        "test",
        "test",
        "test-account",
        None,
    )
    .await;

    match result {
        Ok(LoggedIn::Plain(_)) => {}
        Ok(LoggedIn::Tls(_)) => panic!("plain login should not return Tls variant"),
        Err(e) => panic!("plain login should succeed: {e}"),
    }
}
