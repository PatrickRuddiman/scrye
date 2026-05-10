//! `SO_PEERCRED` access control. Even though the kernel rejects
//! non-owner connections at the FS-permission layer (the socket file is
//! mode `0600`), this module backstops with an accept-time uid compare
//! and emits the spec's `non-owner-user connection rejection` log line.

use std::sync::OnceLock;

use scryd_log::{category, log_failure};
use tokio::net::UnixStream;

use crate::ApiError;

/// Name of the env var that overrides the daemon's expected peer uid.
/// systemd's unit file sets this to the operator's uid so the daemon
/// (running as `scryd:scryd`) accepts connections from `<operator>`.
pub const ENV_ALLOWED_UID: &str = "SCRYD_ALLOWED_UID";

static EXPECTED_UID: OnceLock<u32> = OnceLock::new();

/// Read `SCRYD_ALLOWED_UID` and return the parsed peer uid the daemon
/// should accept. Empty / unset → fall back to the running process's
/// own uid (matches v0.1.0 behaviour). A non-empty unparseable value
/// fails fast with [`ApiError::AllowedUidParse`] so systemd surfaces
/// a clear cause when the unit fails to start.
pub fn expected_peer_uid() -> Result<u32, ApiError> {
    match std::env::var(ENV_ALLOWED_UID) {
        Ok(s) if !s.is_empty() => s
            .parse::<u32>()
            .map_err(|_| ApiError::AllowedUidParse { raw: s }),
        _ => Ok(current_uid()),
    }
}

/// Eagerly resolve and cache the expected peer uid. Call at daemon
/// startup before opening the listener so a malformed env var fails
/// the unit immediately. Idempotent: subsequent calls are no-ops.
pub fn init() -> Result<(), ApiError> {
    let resolved = expected_peer_uid()?;
    let _ = EXPECTED_UID.set(resolved);
    Ok(())
}

fn cached_expected_uid() -> u32 {
    *EXPECTED_UID.get_or_init(|| expected_peer_uid().unwrap_or_else(|_| current_uid()))
}

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
/// is `true`, falls back to the v0.2.0 single-operator-host model.
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
    let expected = cached_expected_uid();
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
