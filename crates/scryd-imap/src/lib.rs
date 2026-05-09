//! IMAP client wrapper around `async-imap`. Pinned to a strictly-read-only
//! command set via the [`verbs::ImapVerb`] enum; every wire emission goes
//! through [`verbs::assert_verb_allowed`] before reaching the underlying
//! library.
//!
//! Tasks 13–15 layer the per-(account, folder) state machine, IDLE/poll
//! handling, tombstone scan, and the scheduler on top of this crate.

pub mod capabilities;
pub mod client;
pub mod connect;
pub mod credential;
pub mod fetch;
pub mod idle;
pub mod poll;
pub mod scheduler;
pub mod sink;
pub mod state;
pub mod supervisor;
pub mod plain;
pub mod tls;
pub mod tombstone;
pub mod uidvalidity;
pub mod verbs;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("connect failed: {0}")]
    Connect(String),
    #[error("tls handshake failed: {0}")]
    TlsHandshake(String),
    #[error("server greeting / protocol error: {0}")]
    Server(String),
    #[error("authentication rejected by server")]
    AuthRejected,
    #[error("a future code change passed an unallowed IMAP verb to the wire wrapper")]
    MutatingCommandRejected,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub use capabilities::Capabilities;
pub use client::Client;
pub use connect::{login, login_plain, login_tls, LoggedIn};
pub use fetch::{fetch_batch, run_incremental, run_initial_backfill, FETCH_ATTRS};
pub use idle::{
    emit_idle_drop, emit_idle_enter, emit_idle_recycle, run_idle_loop, IDLE_RECYCLE,
};
pub use poll::{effective_poll_interval, run_poll_loop, POLL_INTERVAL_FLOOR};
pub use credential::{ConfigCredentialFetcher, CredentialFetcher};
pub use scheduler::{
    backoff_after, BACKOFF_CAP, INITIAL_BACKOFF, MAX_CONNECTIONS_PER_ACCOUNT,
};
pub use supervisor::{
    log_auth_rejection, log_connect_failure, log_push_channel_drop, log_tls_failure,
};
pub use tombstone::{compute_tombstones, scan as tombstone_scan, TOMBSTONE_SCAN_EVERY};
pub use sink::{FetchedMessage, MessageSink, SyncStateUpdate};
pub use state::{ConnState, Connection, INCREMENTAL_FETCH_BATCH};
pub use uidvalidity::handle_change as handle_uidvalidity_change;
pub use verbs::{assert_verb_allowed, ImapVerb};
