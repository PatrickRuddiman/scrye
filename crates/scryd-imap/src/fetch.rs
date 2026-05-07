//! UID-keyed FETCH orchestration. Issues
//! `UID FETCH <range> (UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])`
//! and surfaces results as [`FetchedMessage`] values.
//!
//! Live integration against `async-imap` is deferred to a follow-up that
//! authors the mock IMAP server harness; this module pins the const
//! string and the function signature so tasks 14–15 can build against it.

use futures::{AsyncRead, AsyncWrite};

use crate::client::Client;
use crate::sink::FetchedMessage;
use crate::ClientError;

/// FETCH attribute list scryd issues for every batch — exhaustively
/// read-only; `BODY.PEEK[]` returns the full RFC 5322 payload without
/// setting the `\Seen` flag.
pub const FETCH_ATTRS: &str = "UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[]";

/// Fetch one chunk of messages by UID range. Live response parsing is
/// authored in a follow-up; this signature exists so the orchestration
/// layers (initial backfill, incremental, idle wake) compile against it.
pub async fn fetch_batch<S>(
    _client: &mut Client<S>,
    _range: &str,
    _account_id: &str,
    _folder: &str,
    _uidvalidity: u32,
) -> Result<Vec<FetchedMessage>, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    // Implementation chain (deferred):
    //   client.uid_fetch(range, FETCH_ATTRS) -> response stream
    //   for each response, parse UID/FLAGS/INTERNALDATE/raw bytes
    //   collect into Vec<FetchedMessage>
    //   return
    //
    // Reason for deferral: building a hand-rolled async IMAP mock server
    // to drive this end-to-end is itself a task (the test fixture is
    // larger than the parsing code). The state machine + sink interface
    // ship now so tasks 14–15 stay unblocked.
    Err(ClientError::Server(
        "fetch_batch live integration deferred; see crates/scryd-imap/src/fetch.rs".into(),
    ))
}
