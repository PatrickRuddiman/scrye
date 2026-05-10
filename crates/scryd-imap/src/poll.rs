//! Poll-loop helpers + the loop body. Used when the server doesn't
//! advertise IDLE; the loop sleeps `poll_interval` between
//! incremental fetches.

use std::time::Duration;

use futures::{AsyncRead, AsyncWrite};
use tokio::sync::watch;

use crate::client::Client;
use crate::fetch::run_incremental;
use crate::sink::MessageSink;
use crate::state::{ConnState, Connection};
use crate::tombstone::{scan as tombstone_scan, TOMBSTONE_SCAN_EVERY};
use crate::ClientError;

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

/// Drive the poll loop until the shutdown signal fires. Sleep
/// `poll_interval`, then `run_incremental(.., last_seen_uid)`,
/// advancing the watermark each iteration. The integration test
/// passes a sub-floor interval so the loop ticks fast enough to
/// observe in test budget; production callers should use
/// [`effective_poll_interval`] to honor the floor.
pub async fn run_poll_loop<S>(
    conn: &mut Connection,
    client: Client<S>,
    sink: &dyn MessageSink,
    initial_last_seen_uid: u32,
    poll_interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(Client<S>, u32), ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let mut current_client = client;
    let mut last_seen_uid = initial_last_seen_uid;
    let mut iteration: u32 = 0;
    let mut tombstone_uidvalidity: u32 = sink
        .stored_uidvalidity(&conn.account_id, &conn.folder)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);
    conn.state = ConnState::Polling;

    loop {
        if *shutdown.borrow() {
            return Ok((current_client, last_seen_uid));
        }
        tokio::select! {
            _ = shutdown.changed() => return Ok((current_client, last_seen_uid)),
            _ = tokio::time::sleep(poll_interval) => {}
        }

        iteration = iteration.saturating_add(1);
        if iteration > 1 && iteration % TOMBSTONE_SCAN_EVERY == 0 && tombstone_uidvalidity > 0 {
            let local_uids = sink
                .list_local_uids(&conn.account_id, &conn.folder, tombstone_uidvalidity)
                .await
                .unwrap_or_default();
            let _ = tombstone_scan(conn, &mut current_client, sink, &local_uids).await;
        }

        let new_uid = run_incremental(conn, &mut current_client, sink, last_seen_uid).await?;
        if new_uid > last_seen_uid {
            last_seen_uid = new_uid;
        }
        tombstone_uidvalidity = sink
            .stored_uidvalidity(&conn.account_id, &conn.folder)
            .await
            .ok()
            .flatten()
            .unwrap_or(tombstone_uidvalidity);
    }
}
