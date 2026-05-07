//! IDLE-loop helpers. The loop body itself depends on a live IMAP server
//! and is deferred to a follow-up; this module pins the cadence constant
//! and the lifecycle-log emissions referenced from the task ACs.

use std::time::Duration;

use scryd_log::{kind, log_lifecycle};

/// Cadence at which the IDLE channel is recycled. RFC 2177 recommends
/// 29 minutes; we round down to 25 to give a safe margin against
/// servers (Gmail in particular) that close the channel earlier.
pub const IDLE_RECYCLE: Duration = Duration::from_secs(25 * 60);

/// Emit the lifecycle event when a connection enters IDLE.
pub fn emit_idle_enter(account_id: &str, folder: &str) {
    log_lifecycle!(
        kind = kind::IDLE_ENTER,
        account_id = %account_id,
        folder = %folder
    );
}

/// Emit the lifecycle event when an IDLE channel drops (server closed it,
/// or the supervisor took it down).
pub fn emit_idle_drop(account_id: &str, folder: &str, reason: &str) {
    log_lifecycle!(
        kind = kind::IDLE_DROP,
        account_id = %account_id,
        folder = %folder,
        reason = %reason
    );
}

/// Emit the lifecycle event when the IDLE channel is voluntarily recycled
/// at the [`IDLE_RECYCLE`] interval.
pub fn emit_idle_recycle(account_id: &str, folder: &str) {
    log_lifecycle!(
        kind = kind::IDLE_RECYCLE,
        account_id = %account_id,
        folder = %folder
    );
}
