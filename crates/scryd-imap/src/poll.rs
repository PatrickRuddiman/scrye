//! Poll-loop helpers. Like `idle.rs`, the loop body itself is deferred;
//! this module pins the cadence math the scheduler uses.

use std::time::Duration;

/// The minimum poll interval the scheduler honors. Operators can configure
/// a larger interval via `[sync] poll_interval_seconds`; this floor exists
/// to keep accidentally-tiny values (e.g. `0`) from busy-looping.
pub const POLL_INTERVAL_FLOOR: Duration = Duration::from_secs(30);

/// Resolve the effective poll interval given the operator-configured value
/// (in seconds). Values below the floor are clamped up.
pub fn effective_poll_interval(configured_secs: u32) -> Duration {
    let configured = Duration::from_secs(configured_secs as u64);
    if configured < POLL_INTERVAL_FLOOR {
        POLL_INTERVAL_FLOOR
    } else {
        configured
    }
}
