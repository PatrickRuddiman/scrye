//! Per-(account, folder) supervisor failure-log helpers. Every error path
//! a supervisor takes routes through one of these so the spec's
//! closed-set categories stay grep-stable.

use scryd_log::{category, log_failure};

/// Log a TCP connect-time failure (no host, network unreachable, etc.).
pub fn log_connect_failure(account_id: &str, error: &str) {
    log_failure!(
        category = category::CONNECT_FAILURE,
        account_id = %account_id,
        error = %error
    );
}

/// Log a TLS handshake failure (bad cert, no shared cipher, etc.).
pub fn log_tls_failure(account_id: &str, error: &str) {
    log_failure!(
        category = category::TLS_FAILURE,
        account_id = %account_id,
        error = %error
    );
}

/// Log an IMAP authentication rejection.
pub fn log_auth_rejection(account_id: &str, error: &str) {
    log_failure!(
        category = category::AUTH_REJECTION,
        account_id = %account_id,
        error = %error
    );
}

/// Log an IDLE / push-channel drop (server-initiated close, network
/// hiccup that took out the IDLE socket, etc.).
pub fn log_push_channel_drop(account_id: &str, folder: &str, reason: &str) {
    log_failure!(
        severity = warn,
        category = category::PUSH_CHANNEL_DROP,
        account_id = %account_id,
        folder = %folder,
        reason = %reason
    );
}
