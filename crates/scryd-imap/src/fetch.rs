//! UID-keyed FETCH orchestration. Issues
//! `UID FETCH <range> (UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])`
//! and surfaces results as [`FetchedMessage`] values.

use futures::{AsyncRead, AsyncWrite, StreamExt};

use crate::client::Client;
use crate::sink::{FetchedMessage, MessageSink, SyncStateUpdate};
use crate::state::{ConnState, Connection, INCREMENTAL_FETCH_BATCH};
use crate::verbs::{assert_verb_allowed, ImapVerb};
use crate::ClientError;

/// FETCH attribute list scryd issues for every batch — exhaustively
/// read-only; `BODY.PEEK[]` returns the full RFC 5322 payload without
/// setting the `\Seen` flag. Parenthesised because async-imap passes
/// the query string verbatim into the FETCH command.
pub const FETCH_ATTRS: &str =
    "(UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])";

/// Fetch one chunk of messages by UID range. Each [`FetchedMessage`]
/// carries the caller's `account_id` / `folder` / `uidvalidity`
/// alongside the wire-side UID, internal date, flags, and raw body.
pub async fn fetch_batch<S>(
    client: &mut Client<S>,
    range: &str,
    account_id: &str,
    folder: &str,
    uidvalidity: u32,
) -> Result<Vec<FetchedMessage>, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    assert_verb_allowed(&ImapVerb::UidFetch)?;
    let session = client.session_mut();
    let mut stream = session
        .uid_fetch(range, FETCH_ATTRS)
        .await
        .map_err(|e| ClientError::Server(e.to_string()))?;

    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        let fetch = item.map_err(|e| ClientError::Server(e.to_string()))?;
        out.push(parse_fetch_row(&fetch, account_id, folder, uidvalidity)?);
    }
    Ok(out)
}

/// Walk a folder from UID 1 up through the server's UIDNEXT-1, in
/// `INCREMENTAL_FETCH_BATCH`-sized batches, handing every fetched
/// message to the sink. Updates `sink.update_sync_state` after each
/// batch so a SIGKILL mid-walk resumes from a recent watermark on
/// the next start.
pub async fn run_initial_backfill<S>(
    conn: &mut Connection,
    client: &mut Client<S>,
    sink: &dyn MessageSink,
) -> Result<(), ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let meta = client.examine_meta(&conn.folder).await?;
    let server_uidvalidity = meta.uid_validity.ok_or_else(|| {
        ClientError::Server(format!(
            "EXAMINE {} did not report UIDVALIDITY",
            conn.folder
        ))
    })?;

    // UIDVALIDITY change handling: the sink's stored value (from a
    // previous session) may differ from what the server reports
    // now. Route through the lifecycle-log helper before starting
    // the new backfill so the audit trail names the reset.
    if let Ok(Some(stored)) = sink
        .stored_uidvalidity(&conn.account_id, &conn.folder)
        .await
    {
        if stored != server_uidvalidity {
            crate::uidvalidity::handle_change(conn, sink, server_uidvalidity).await?;
        }
    }

    let target = meta.uid_next.map(|n| n.saturating_sub(1));

    conn.enter_backfill(target);

    let mut lo: u32 = 1;
    let mut max_uid_seen: u32 = 0;
    loop {
        if let Some(t) = target {
            if t == 0 || lo > t {
                break;
            }
        }
        let hi = lo.saturating_add(INCREMENTAL_FETCH_BATCH - 1);
        let range = format!("{lo}:{hi}");
        let batch = fetch_batch(client, &range, &conn.account_id, &conn.folder, server_uidvalidity).await?;
        for fetched in batch {
            let observed = fetched.server_uid;
            if observed > max_uid_seen {
                max_uid_seen = observed;
            }
            sink.submit(fetched).await?;
            conn.advance_backfill(observed);
        }
        sink.update_sync_state(
            &conn.account_id,
            &conn.folder,
            SyncStateUpdate {
                uidvalidity: Some(server_uidvalidity),
                last_seen_uid: Some(max_uid_seen),
                ..Default::default()
            },
        )
        .await?;

        if target.map(|t| hi >= t).unwrap_or(false) {
            break;
        }
        lo = hi.saturating_add(1);
    }

    conn.state = ConnState::Idling;
    Ok(())
}

/// Catch up on messages newer than `last_seen_uid` for the connection's
/// folder. Triggered after IDLE wake / poll tick. UIDVALIDITY change
/// routes the caller to `run_initial_backfill` via
/// [`crate::uidvalidity::handle_change`].
pub async fn run_incremental<S>(
    conn: &mut Connection,
    client: &mut Client<S>,
    sink: &dyn MessageSink,
    last_seen_uid: u32,
) -> Result<u32, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let meta = client.examine_meta(&conn.folder).await?;
    let server_uidvalidity = meta.uid_validity.ok_or_else(|| {
        ClientError::Server(format!(
            "EXAMINE {} did not report UIDVALIDITY",
            conn.folder
        ))
    })?;

    let from = last_seen_uid.saturating_add(1);
    let range = format!("{from}:*");
    conn.state = ConnState::Fetching;
    let batch = fetch_batch(client, &range, &conn.account_id, &conn.folder, server_uidvalidity).await?;

    let mut max_uid_seen = last_seen_uid;
    for fetched in batch {
        if fetched.server_uid > max_uid_seen {
            max_uid_seen = fetched.server_uid;
        }
        sink.submit(fetched).await?;
    }
    if max_uid_seen > last_seen_uid {
        sink.update_sync_state(
            &conn.account_id,
            &conn.folder,
            SyncStateUpdate {
                uidvalidity: Some(server_uidvalidity),
                last_seen_uid: Some(max_uid_seen),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(max_uid_seen)
}

/// Map a single async-imap `Fetch` into a [`FetchedMessage`]. Pulled
/// out so a future test can exercise the field extraction without a
/// live IMAP session.
fn parse_fetch_row(
    fetch: &async_imap::types::Fetch,
    account_id: &str,
    folder: &str,
    uidvalidity: u32,
) -> Result<FetchedMessage, ClientError> {
    let server_uid = fetch
        .uid
        .ok_or_else(|| ClientError::Server("FETCH response missing UID".into()))?;
    let internal_date = fetch.internal_date().map(|dt| dt.timestamp());
    let flags: Vec<String> = fetch.flags().map(|f| format!("{f:?}")).collect();
    let raw_bytes: Vec<u8> = fetch.body().map(|b| b.to_vec()).unwrap_or_default();
    Ok(FetchedMessage {
        account_id: account_id.to_string(),
        folder: folder.to_string(),
        server_uid,
        uidvalidity,
        internal_date,
        flags,
        raw_bytes,
    })
}
