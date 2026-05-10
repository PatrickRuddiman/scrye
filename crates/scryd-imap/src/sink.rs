//! [`MessageSink`] — the interface the IMAP fetch path hands every fetched
//! message to. The runtime crate (task 16) adapts a `MessageSink` impl
//! that drives MIME parsing and writes through scryd-storage; this crate
//! stays storage-agnostic.

use async_trait::async_trait;

use crate::ClientError;

/// Raw bytes plus sync metadata for a message just fetched from IMAP.
#[derive(Debug, Clone)]
pub struct FetchedMessage {
    pub account_id: String,
    pub folder: String,
    pub server_uid: u32,
    pub uidvalidity: u32,
    /// IMAP `INTERNALDATE` as unix seconds, when the server reported one.
    pub internal_date: Option<i64>,
    /// IMAP flags as raw strings (informational only — scryd never writes
    /// them back).
    pub flags: Vec<String>,
    /// Full `BODY.PEEK[]` payload — the raw RFC 5322 message bytes.
    pub raw_bytes: Vec<u8>,
}

/// Partial update to a `(account_id, folder)` sync state row. Only fields
/// that are `Some` get written; the rest are preserved.
#[derive(Debug, Clone, Default)]
pub struct SyncStateUpdate {
    pub uidvalidity: Option<u32>,
    pub last_seen_uid: Option<u32>,
    pub last_full_sync_at: Option<i64>,
    pub last_idle_at: Option<i64>,
    /// Outer `Option` flags whether to write at all; inner is the value.
    pub last_error: Option<Option<String>>,
    pub backoff_until: Option<Option<i64>>,
    pub account_health: Option<String>,
}

/// Trait the IMAP fetch path consumes. Concrete impl lives in the runtime
/// crate (task 16); a stub impl shows up in this crate's tests so the
/// interface compiles in isolation.
#[async_trait]
pub trait MessageSink: Send + Sync {
    async fn submit(&self, fetched: FetchedMessage) -> Result<(), ClientError>;
    async fn tombstone(&self, message_id: &str) -> Result<(), ClientError>;
    async fn update_sync_state(
        &self,
        account_id: &str,
        folder: &str,
        update: SyncStateUpdate,
    ) -> Result<(), ClientError>;
    /// Return the locally-stored UID set for `(account_id, folder,
    /// uidvalidity)`. The supervisor uses this to drive the periodic
    /// tombstone-scan diff against the server's `UID SEARCH ALL`.
    /// Default impl returns an empty Vec so existing test stubs that
    /// don't override compile cleanly.
    async fn list_local_uids(
        &self,
        _account_id: &str,
        _folder: &str,
        _uidvalidity: u32,
    ) -> Result<Vec<u32>, ClientError> {
        Ok(Vec::new())
    }
    /// Return the stored uidvalidity for `(account_id, folder)`. The
    /// supervisor uses this to detect mid-session UIDVALIDITY change
    /// and re-route through `uidvalidity::handle_change`. Default
    /// impl returns None (no stored state) so the supervisor treats
    /// any first-seen value as a fresh open.
    async fn stored_uidvalidity(
        &self,
        _account_id: &str,
        _folder: &str,
    ) -> Result<Option<u32>, ClientError> {
        Ok(None)
    }
}
