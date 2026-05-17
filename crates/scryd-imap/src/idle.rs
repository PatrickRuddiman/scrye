//! IDLE-loop helpers + the loop body. Per RFC 2177: enter IDLE, wait
//! for an EXISTS/RECENT/FETCH event from the server, send DONE, run
//! an incremental fetch, re-enter IDLE. Recycle the channel every
//! [`IDLE_RECYCLE`] to dodge servers that close idle connections.

use std::time::Duration;

use futures::{AsyncRead, AsyncWrite};
use scryd_log::{kind, log_lifecycle};
use tokio::sync::watch;

use crate::client::Client;
use crate::fetch::run_incremental;
use crate::sink::{MessageSink, SyncStateUpdate};
use crate::state::{ConnState, Connection};
use crate::tombstone::{scan as tombstone_scan, TOMBSTONE_SCAN_EVERY};
use crate::ClientError;

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

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

enum IdleOutcome {
    Event,
    Recycle,
    Shutdown,
}

/// Drive the IDLE loop until the shutdown signal fires. Returns the
/// reclaimed [`Client`] and the most recent watermark so the caller
/// can persist it / hand it to a subsequent reconnect.
///
/// Each iteration:
/// 1. EXAMINE-already-selected → IDLE start (transitions conn to Idling).
/// 2. Race the IDLE wait against `idle_recycle` and the shutdown signal.
/// 3. Send DONE.
/// 4. On event: `run_incremental(.., last_seen_uid)`; advance watermark.
/// 5. On recycle: emit lifecycle log, no fetch.
/// 6. On shutdown: return.
pub async fn run_idle_loop<S>(
    conn: &mut Connection,
    client: Client<S>,
    sink: &dyn MessageSink,
    initial_last_seen_uid: u32,
    idle_recycle: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(Client<S>, u32), ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let mut current_client = client;
    let mut last_seen_uid = initial_last_seen_uid;
    let mut iteration: u32 = 0;
    // The uidvalidity context for tombstone scans is read from the
    // sink's stored value — populated during the most recent
    // run_initial_backfill / run_incremental update_sync_state call.
    let mut tombstone_uidvalidity: u32 = sink
        .stored_uidvalidity(&conn.account_id, &conn.folder)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);

    loop {
        if *shutdown.borrow() {
            return Ok((current_client, last_seen_uid));
        }

        // Every TOMBSTONE_SCAN_EVERY iterations, run the scan before
        // re-entering IDLE. iteration=0 skips the first scan (nothing
        // useful to compare yet).
        iteration = iteration.saturating_add(1);
        if iteration > 1 && iteration % TOMBSTONE_SCAN_EVERY == 0 && tombstone_uidvalidity > 0 {
            let local_uids = sink
                .list_local_uids(&conn.account_id, &conn.folder, tombstone_uidvalidity)
                .await
                .unwrap_or_default();
            let _ = tombstone_scan(conn, &mut current_client, sink, &local_uids).await;
        }

        conn.state = ConnState::Idling;
        let mut handle = current_client.idle_start().await?;
        emit_idle_enter(&conn.account_id, &conn.folder);

        let outcome: Result<IdleOutcome, ClientError> = tokio::select! {
            _ = shutdown.changed() => Ok(IdleOutcome::Shutdown),
            r = wait_for_idle_event(&mut handle, idle_recycle) => r,
        };

        let session = handle
            .done()
            .await
            .map_err(|e| ClientError::Server(e.to_string()))?;
        current_client = Client::from_session(session);

        match outcome? {
            IdleOutcome::Shutdown => {
                emit_idle_drop(&conn.account_id, &conn.folder, "shutdown");
                return Ok((current_client, last_seen_uid));
            }
            IdleOutcome::Recycle => {
                emit_idle_recycle(&conn.account_id, &conn.folder);
                // Refresh the freshness timestamp so /status doesn't go
                // stale on a quiet account that's just sitting in IDLE.
                let _ = sink
                    .update_sync_state(
                        &conn.account_id,
                        &conn.folder,
                        SyncStateUpdate {
                            last_full_sync_at: Some(now_unix()),
                            ..Default::default()
                        },
                    )
                    .await;
            }
            IdleOutcome::Event => {
                let new_uid =
                    run_incremental(conn, &mut current_client, sink, last_seen_uid).await?;
                if new_uid > last_seen_uid {
                    last_seen_uid = new_uid;
                }
                // Refresh the uidvalidity used by future tombstone
                // scans — run_incremental's update_sync_state may
                // have advanced it.
                tombstone_uidvalidity = sink
                    .stored_uidvalidity(&conn.account_id, &conn.folder)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(tombstone_uidvalidity);
            }
        }
    }
}

async fn wait_for_idle_event<S>(
    handle: &mut async_imap::extensions::idle::Handle<S>,
    idle_recycle: Duration,
) -> Result<IdleOutcome, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let (wait_future, _stop) = handle.wait();
    match tokio::time::timeout(idle_recycle, wait_future).await {
        Ok(Ok(_response)) => Ok(IdleOutcome::Event),
        Ok(Err(e)) => Err(ClientError::Server(e.to_string())),
        Err(_) => Ok(IdleOutcome::Recycle),
    }
}
