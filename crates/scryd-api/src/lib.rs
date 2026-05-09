//! axum router and Unix-domain-socket transport. The peercred guard
//! ensures only processes running as the daemon's owning uid can reach
//! the API; everything else is blocked at accept time.
//!
//! Tasks 18–19 add the public read endpoints (`/search`, `/message/:id`,
//! `/thread/:id`, `/accounts`, `/message/:id/raw`) and the write endpoints
//! (`/sync`, `/internal/reindex`, `/internal/reconcile`) on top of this
//! crate's scaffolding.

pub mod dto;
pub mod handlers;
pub mod handlers_write;
pub mod peercred;
pub mod response;
pub mod router;
pub mod serve;
pub mod socket;
pub mod state;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("UDS bind: {0}")]
    Bind(#[source] std::io::Error),
    #[error("scryd is already running on {path}")]
    AlreadyRunning { path: std::path::PathBuf },
    #[error("connecting peer uid {peer_uid} is not the daemon's owning uid")]
    NonOwner { peer_uid: u32 },
    #[error("SCRYD_ALLOWED_UID must be a non-negative integer; got `{raw}`")]
    AllowedUidParse { raw: String },
    #[error("peer credentials unavailable: {0}")]
    PeerCred(#[source] std::io::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub use peercred::{check_peer_uid, expected_peer_uid, extract_peer_uid, init as init_peercred};
pub use router::router;
pub use serve::serve;
pub use socket::bind;
pub use state::AppState;
