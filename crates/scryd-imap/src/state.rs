//! Per-(account, folder) connection state machine. Pure data types; the
//! state transitions live in the higher-level orchestration in `fetch.rs`,
//! `idle.rs` (task 14), and `scheduler.rs` (task 15).

/// Maximum UIDs requested per `UID FETCH` issued during initial backfill
/// or large incremental catch-up. Bounds memory and per-request wall time
/// so a SIGKILL mid-batch resumes from a recent watermark.
pub const INCREMENTAL_FETCH_BATCH: u32 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    Disconnected,
    Resolving,
    Connecting,
    TlsHandshaking,
    LoggingIn,
    CapabilityChecking,
    Selecting,
    InitialBackfilling { last_uid: u32, target: Option<u32> },
    Idling,
    Polling,
    Fetching,
    Backoff { until_unix_secs: i64 },
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub account_id: String,
    pub folder: String,
    pub state: ConnState,
}

impl Connection {
    pub fn new(account_id: impl Into<String>, folder: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            folder: folder.into(),
            state: ConnState::Disconnected,
        }
    }

    /// Transition into the InitialBackfilling state, resetting the watermark.
    pub fn enter_backfill(&mut self, target: Option<u32>) {
        self.state = ConnState::InitialBackfilling {
            last_uid: 0,
            target,
        };
    }

    /// Advance the InitialBackfilling watermark to `new_last_uid`. No-op if
    /// the connection isn't currently backfilling.
    pub fn advance_backfill(&mut self, new_last_uid: u32) {
        if let ConnState::InitialBackfilling { last_uid, .. } = &mut self.state {
            if new_last_uid > *last_uid {
                *last_uid = new_last_uid;
            }
        }
    }

    /// Mark the connection as in backoff until the given unix epoch second.
    pub fn enter_backoff(&mut self, until_unix_secs: i64) {
        self.state = ConnState::Backoff { until_unix_secs };
    }

    /// Returns true if the connection is in any state that consumes server
    /// resources (a TCP connection, a TLS session, or an open mailbox).
    pub fn is_connected(&self) -> bool {
        matches!(
            self.state,
            ConnState::CapabilityChecking
                | ConnState::Selecting
                | ConnState::InitialBackfilling { .. }
                | ConnState::Idling
                | ConnState::Polling
                | ConnState::Fetching
        )
    }
}
