//! Tombstone-scan diff math + live orchestration. Every Nth incremental
//! pass, the scheduler runs `UID SEARCH ALL` and compares the returned
//! UID set against the local set; UIDs locally present but server-absent
//! are tombstoned.

use std::collections::BTreeSet;

use futures::{AsyncRead, AsyncWrite};

use crate::client::Client;
use crate::sink::MessageSink;
use crate::state::Connection;
use crate::ClientError;

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

/// Live orchestration: issue `UID SEARCH ALL` against the open
/// mailbox, diff against the caller-provided `local_uids`, and call
/// `sink.tombstone(uid_string)` for each locally-present-but-server-
/// absent UID. Returns the count tombstoned. The caller (scheduler)
/// supplies `local_uids` from storage so this function stays
/// storage-agnostic.
pub async fn scan<S>(
    conn: &Connection,
    client: &mut Client<S>,
    sink: &dyn MessageSink,
    local_uids: &[u32],
) -> Result<u32, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let server_uids = client.uid_search("ALL").await?;
    let to_tombstone = compute_tombstones(local_uids, &server_uids);
    for uid in &to_tombstone {
        let id = format!("{}/{}/{}", conn.account_id, conn.folder, uid);
        sink.tombstone(&id).await?;
    }
    Ok(to_tombstone.len() as u32)
}
