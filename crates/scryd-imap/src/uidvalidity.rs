//! UIDVALIDITY change recovery. When the server reports a different
//! UIDVALIDITY than the one storage has on file, the per-folder UID
//! watermark is meaningless and must be reset; previously-stored
//! `messages` rows from the old epoch remain queryable.

use scryd_log::{kind, log_lifecycle};

use crate::sink::{MessageSink, SyncStateUpdate};
use crate::state::{ConnState, Connection};
use crate::ClientError;

/// React to a UIDVALIDITY change for the connection's (account, folder).
/// Emits a `kind::UIDVALIDITY_RESET` lifecycle event, clears the
/// last_seen_uid via the sink, and transitions the connection back into
/// InitialBackfilling so the next pass re-syncs the folder from UID 1.
pub async fn handle_change(
    conn: &mut Connection,
    sink: &dyn MessageSink,
    new_uidvalidity: u32,
) -> Result<(), ClientError> {
    log_lifecycle!(
        kind = kind::UIDVALIDITY_RESET,
        account_id = %conn.account_id,
        folder = %conn.folder,
        new_uidvalidity = new_uidvalidity
    );

    sink.update_sync_state(
        &conn.account_id,
        &conn.folder,
        SyncStateUpdate {
            uidvalidity: Some(new_uidvalidity),
            last_seen_uid: Some(0),
            ..Default::default()
        },
    )
    .await?;

    conn.state = ConnState::InitialBackfilling {
        last_uid: 0,
        target: None,
    };
    Ok(())
}
