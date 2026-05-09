//! UID-keyed FETCH orchestration. Issues
//! `UID FETCH <range> (UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])`
//! and surfaces results as [`FetchedMessage`] values.

use futures::{AsyncRead, AsyncWrite, StreamExt};

use crate::client::Client;
use crate::sink::FetchedMessage;
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
