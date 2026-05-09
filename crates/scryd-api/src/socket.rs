//! Unix-domain-socket bind. Creates `<runtime_dir>/scryd/` mode `0700` if
//! missing, recovers from a stale socket file left by a crashed prior
//! daemon, and binds the socket mode `0600`. Returns
//! [`ApiError::AlreadyRunning`] if a live daemon is already listening on
//! the path.

use std::path::Path;

use tokio::net::{UnixListener, UnixStream};

use crate::ApiError;

/// Bind the daemon's API socket. The argument should already include the
/// `/scryd` suffix (per `scryd_runtime::xdg::runtime_dir`).
pub async fn bind(scryd_runtime_dir: &Path) -> Result<UnixListener, ApiError> {
    if !scryd_runtime_dir.exists() {
        std::fs::create_dir_all(scryd_runtime_dir).map_err(ApiError::Bind)?;
        tighten_dir_perms(scryd_runtime_dir)?;
    }
    // If the dir already exists, trust whatever provisioned it: v0.1.0
    // had the operator mkdir at 0700; v0.2.0's tmpfiles drop-in sets
    // scryd:operator 0750. The bind path must not regress operator
    // group access on the v0.2.0 layout.

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
    // v0.2.0: socket inherits the directory's group so the operator
    // (group member) can connect while non-group users are blocked
    // at the kernel layer regardless of the peercred backstop. Mode
    // 0660 + dir-inherited group is the kernel-level enforcement;
    // the SO_PEERCRED check is the second defense.
    inherit_dir_group(&socket_path, scryd_runtime_dir)?;
    set_socket_mode_0660(&socket_path)?;
    Ok(listener)
}

#[cfg(unix)]
fn inherit_dir_group(socket: &Path, dir: &Path) -> Result<(), ApiError> {
    use std::os::unix::fs::MetadataExt;
    let dir_meta = std::fs::metadata(dir).map_err(ApiError::Bind)?;
    let dir_gid = nix::unistd::Gid::from_raw(dir_meta.gid());
    nix::unistd::chown(socket, None, Some(dir_gid))
        .map_err(|e| ApiError::Bind(std::io::Error::other(e.to_string())))
}

#[cfg(not(unix))]
fn inherit_dir_group(_socket: &Path, _dir: &Path) -> Result<(), ApiError> {
    Ok(())
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
fn set_socket_mode_0660(path: &Path) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))
        .map_err(ApiError::Bind)
}

#[cfg(not(unix))]
fn set_socket_mode_0660(_path: &Path) -> Result<(), ApiError> {
    Ok(())
}
