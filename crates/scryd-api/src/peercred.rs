//! `SO_PEERCRED` access control. Even though the kernel rejects
//! non-owner connections at the FS-permission layer (the socket file is
//! mode `0600`), this module backstops with an accept-time uid compare
//! and emits the spec's `non-owner-user connection rejection` log line.

use scryd_log::{category, log_failure};
use tokio::net::UnixStream;

use crate::ApiError;

/// Read the connecting peer's uid via `getsockopt(SO_PEERCRED)`.
pub fn extract_peer_uid(stream: &UnixStream) -> Result<u32, ApiError> {
    let creds = stream.peer_cred().map_err(ApiError::PeerCred)?;
    Ok(creds.uid())
}

/// Compare the peer's uid against the daemon's expected uid. On mismatch,
/// emit the spec's `non-owner-user connection rejection` log line and
/// return [`ApiError::NonOwner`].
pub fn check_peer_uid(peer_uid: u32, expected_uid: u32, peer_pid: i32) -> Result<(), ApiError> {
    if peer_uid == expected_uid {
        return Ok(());
    }
    log_failure!(
        category = category::NON_OWNER_REJECTION,
        peer_uid = peer_uid,
        peer_pid = peer_pid
    );
    Err(ApiError::NonOwner { peer_uid })
}

/// Convenience: read peer uid + check in one call against the running
/// process's uid.
pub fn check_stream_peer(stream: &UnixStream) -> Result<u32, ApiError> {
    let peer_uid = extract_peer_uid(stream)?;
    let peer_pid = stream
        .peer_cred()
        .ok()
        .and_then(|c| c.pid())
        .unwrap_or(-1);
    let expected = current_uid();
    check_peer_uid(peer_uid, expected, peer_pid)?;
    Ok(peer_uid)
}

#[cfg(unix)]
fn current_uid() -> u32 {
    nix::unistd::getuid().as_raw()
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}
