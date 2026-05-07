//! Per-account supervision constants and backoff math. The full Scheduler
//! struct that wires storage + sink + config and spawns supervisor tasks
//! lives in a follow-up alongside the mock IMAP server harness; this
//! module ships the constants and the pure backoff function so other
//! crates depending on the cadence numbers stay honest.

use std::time::Duration;

/// Maximum concurrent IMAP connections per account. Most providers cap
/// concurrent sessions per account (Gmail at 15, Outlook at 20, smaller
/// hosts often lower); 5 is a conservative default.
pub const MAX_CONNECTIONS_PER_ACCOUNT: usize = 5;

/// First retry delay after a failed account connection.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(30);

/// Cap on the exponential retry delay.
pub const BACKOFF_CAP: Duration = Duration::from_secs(3600);

/// Compute the next backoff delay given the number of consecutive failures.
/// Doubles each attempt, capped at [`BACKOFF_CAP`].
///
///   `backoff_after(0)` = 30s   (first failure)
///   `backoff_after(5)` = 960s  (2^5 * 30)
///   `backoff_after(7)` = 3600s (min(2^7 * 30, 3600) = capped)
pub fn backoff_after(failures: u32) -> Duration {
    // Cap the exponent so the shift never overflows. 2^20 * 30s is
    // already 17.8 hours — well past the cap.
    let exp = failures.min(20);
    let secs = 30u64.saturating_mul(1u64 << exp);
    let capped = secs.min(BACKOFF_CAP.as_secs());
    Duration::from_secs(capped)
}
