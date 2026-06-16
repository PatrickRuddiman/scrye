//! axum router and Unix-domain-socket transport. The socket is open
//! by default (v0.3.1 service shape); set `[server] require_peer_uid =
//! true` to gate accept on the daemon's own uid (peercred backstop).

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
    #[error("peer credentials unavailable: {0}")]
    PeerCred(#[source] std::io::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub use peercred::{check_peer_uid, extract_peer_uid};
pub use router::router;
pub use serve::serve;
pub use socket::bind;
pub use state::{AppState, DaemonHealthSnapshot};
