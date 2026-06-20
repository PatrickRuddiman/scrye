//! Daemon-startup self-checks. Validates the per-user filesystem layout
//! the multi-instance-isolation slice's §3 Decision 9 enumerates before
//! any tokio runtime or sqlite open happens.

use std::path::{Path, PathBuf};

use scryd_log::{category, log_failure};

use crate::xdg;
use crate::RuntimeError;

/// Resolved paths the daemon's serve loop consumes after preflight passes.
#[derive(Debug, Clone)]
pub struct PreflightOk {
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
}

/// Run every startup invariant. Each failure logs a
/// `configuration permission error` event tagged with the offending path
/// and exits non-zero through the `?`-propagated error.
pub fn run() -> Result<PreflightOk, RuntimeError> {
    let config = xdg::config_path()?;
    let data = xdg::data_dir()?;
    let assets = xdg::assets_dir()?;

    // Config file: must exist and not be world-readable. v0.3.1 ships
    // `scryd:scryd 0640` so members of group `scryd` can read it, the
    // daemon itself owns it, and everyone else (including the operator
    // when not in the group) is excluded.
    assert_file_no_world_bits(&config, "config")?;

    // Data dir: create if missing, then assert mode 0700.
    if !data.exists() {
        std::fs::create_dir_all(&data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    assert_dir(&data, 0o700, "data dir")?;

    Ok(PreflightOk {
        config_path: config,
        data_dir: data,
        assets_dir: assets,
    })
}

#[cfg(unix)]
fn assert_dir(path: &Path, expected_mode: u32, label: &str) -> Result<(), RuntimeError> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).map_err(|e| {
        log_failure!(
            category = category::CONFIG_PERMISSION_ERROR,
            path = %path.display(),
            error = %e,
            label = label
        );
        RuntimeError::Io(e)
    })?;
    if !meta.is_dir() {
        let reason = format!("{label}: not a directory");
        log_failure!(
            category = category::CONFIG_PERMISSION_ERROR,
            path = %path.display(),
            label = label,
            reason = %reason
        );
        return Err(RuntimeError::PermissionInvariant {
            path: path.to_path_buf(),
            reason,
        });
    }
    let mode = meta.mode() & 0o777;
    if mode != expected_mode {
        let reason = format!(
            "{label}: expected mode {expected_mode:o}, got {mode:o}"
        );
        log_failure!(
            category = category::CONFIG_PERMISSION_ERROR,
            path = %path.display(),
            label = label,
            reason = %reason
        );
        return Err(RuntimeError::PermissionInvariant {
            path: path.to_path_buf(),
            reason,
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn assert_dir(path: &Path, _mode: u32, label: &str) -> Result<(), RuntimeError> {
    if !path.is_dir() {
        return Err(RuntimeError::PermissionInvariant {
            path: path.to_path_buf(),
            reason: format!("{label}: not a directory"),
        });
    }
    Ok(())
}

#[cfg(unix)]
fn assert_file_no_world_bits(path: &Path, label: &str) -> Result<(), RuntimeError> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).map_err(|e| {
        log_failure!(
            category = category::CONFIG_PERMISSION_ERROR,
            path = %path.display(),
            error = %e,
            label = label
        );
        RuntimeError::Io(e)
    })?;
    let mode = meta.mode() & 0o777;
    if mode & 0o007 != 0 {
        let reason = format!("{label}: world bits set on mode {mode:o}");
        log_failure!(
            category = category::CONFIG_PERMISSION_ERROR,
            path = %path.display(),
            label = label,
            reason = %reason
        );
        return Err(RuntimeError::PermissionInvariant {
            path: path.to_path_buf(),
            reason,
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn assert_file_no_world_bits(path: &Path, label: &str) -> Result<(), RuntimeError> {
    if !path.exists() {
        return Err(RuntimeError::PermissionInvariant {
            path: path.to_path_buf(),
            reason: format!("{label}: missing"),
        });
    }
    Ok(())
}
