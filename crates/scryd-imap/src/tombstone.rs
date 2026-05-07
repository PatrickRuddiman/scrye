//! Tombstone-scan diff math. Every Nth incremental pass, the scheduler
//! runs `UID SEARCH ALL` and compares the returned UID set against the
//! local set; UIDs locally present but server-absent are tombstoned.

use std::collections::BTreeSet;

/// Every Nth incremental pass triggers a tombstone scan.
pub const TOMBSTONE_SCAN_EVERY: u32 = 10;

/// Pure diff function. Given the local set of UIDs scryd has indexed for
/// a (account, folder, uidvalidity) and the set the server reports via
/// `UID SEARCH ALL`, return the locally-present-but-server-absent UIDs
/// in ascending order. Those rows are tombstoned through the sink.
pub fn compute_tombstones(local: &[u32], server: &[u32]) -> Vec<u32> {
    let server_set: BTreeSet<u32> = server.iter().copied().collect();
    let mut diff: Vec<u32> = local
        .iter()
        .copied()
        .filter(|uid| !server_set.contains(uid))
        .collect();
    diff.sort_unstable();
    diff.dedup();
    diff
}
