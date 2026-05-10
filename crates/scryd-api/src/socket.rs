//! Unix-domain-socket bind. Creates `<runtime_dir>/scryd/` mode `0700`
//! if missing, recovers from a stale socket file left by a crashed
//! prior daemon, and binds the socket at the caller-supplied mode.
//! Returns [`ApiError::AlreadyRunning`] if a live daemon is already
//! listening on the path.

use std::path::Path;

use tokio::net::{UnixListener, UnixStream};

use crate::ApiError;

/// Bind the daemon's API socket at `socket_mode`. The path argument
/// should already include the `/scryd` suffix (per
/// `scryd_runtime::xdg::runtime_dir`).
///
/// `socket_mode` is the kernel-level access gate. The v0.3.1 default
/// is `0o666` (anyone on the host); v0.2.0's `0o660` is recoverable
/// via `[server] socket_mode = 0o660` in config.
pub async fn bind(scryd_runtime_dir: &Path, socket_mode: u32) -> Result<UnixListener, ApiError> {
    if !scryd_runtime_dir.exists() {
        std::fs::create_dir_all(scryd_runtime_dir).map_err(ApiError::Bind)?;
        tighten_dir_perms(scryd_runtime_dir)?;
    }
    // If the dir already exists, trust whatever provisioned it: the
    // tmpfiles drop-in (or install.sh's manual provision) is the
    // source of truth for the runtime-dir mode.

    let socket_path = scryd_runtime_dir.join("scryd.sock");

    if socket_path.exists() {
        // Stale-socket recovery: try to connect; if a live daemon is on
        // the other side, refuse. Otherwise unlink and rebind.
        match UnixStream::connect(&socket_path).await {
            Ok(_) => {
                return Err(ApiError::AlreadyRunning {
                    path: socket_path,
                });
            }
            Err(_) => {
                std::fs::remove_file(&socket_path).map_err(ApiError::Bind)?;
            }
        }
    }

    let listener = UnixListener::bind(&socket_path).map_err(ApiError::Bind)?;
    set_socket_mode(&socket_path, socket_mode)?;
    Ok(listener)
}

#[cfg(unix)]
fn tighten_dir_perms(dir: &Path) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(ApiError::Bind)
}

#[cfg(not(unix))]
fn tighten_dir_perms(_dir: &Path) -> Result<(), ApiError> {
    Ok(())
}

#[cfg(unix)]
fn set_socket_mode(path: &Path, mode: u32) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(ApiError::Bind)
}

#[cfg(not(unix))]
fn set_socket_mode(_path: &Path, _mode: u32) -> Result<(), ApiError> {
    Ok(())
}
