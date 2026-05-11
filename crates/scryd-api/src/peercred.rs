//! `SO_PEERCRED` access control. v0.3.1 default is "open" (the
//! daemon serves any caller who can reach the socket); enable
//! `[server] require_peer_uid = true` to recover the same-uid-only
//! posture, which is what this module implements.

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
        expected_uid = expected_uid,
        peer_pid = peer_pid
    );
    Err(ApiError::NonOwner { peer_uid })
}

/// Convenience: read peer uid + (optionally) check in one call. When
/// `enabled` is `false`, the peer uid is returned without comparison
/// or rejection — the v0.3.1 default service shape, where access
/// control lives in the consumer's higher-layer API. When `enabled`
/// is `true`, only the daemon's own uid is accepted.
pub fn check_stream_peer(stream: &UnixStream, enabled: bool) -> Result<u32, ApiError> {
    let peer_uid = extract_peer_uid(stream)?;
    if !enabled {
        return Ok(peer_uid);
    }
    let peer_pid = stream
        .peer_cred()
        .ok()
        .and_then(|c| c.pid())
        .unwrap_or(-1);
    check_peer_uid(peer_uid, current_uid(), peer_pid)?;
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
