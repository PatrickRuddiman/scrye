//! Daemon-startup self-checks. Validates the per-user filesystem layout
//! the multi-instance-isolation slice's §3 Decision 9 enumerates before
//! any tokio runtime, sqlite open, or socket bind happens.

use std::path::{Path, PathBuf};

use scryd_log::{category, log_failure};

use crate::xdg;
use crate::RuntimeError;

/// Resolved paths the daemon's serve loop consumes after preflight passes.
#[derive(Debug, Clone)]
pub struct PreflightOk {
    pub runtime_dir: PathBuf,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
}

/// Run every startup invariant. Each failure logs a
/// `configuration permission error` event tagged with the offending path
/// and exits non-zero through the `?`-propagated error.
pub fn run() -> Result<PreflightOk, RuntimeError> {
    let runtime = xdg::runtime_dir()?;
    let config = xdg::config_path()?;
    let data = xdg::data_dir()?;
    let assets = xdg::assets_dir()?;

    // The runtime dir's parent ($XDG_RUNTIME_DIR) is what we audit; the
    // socket subdir is created at bind time. We assert mode 0700 on the
    // parent on Unix.
    let runtime_parent = runtime
        .parent()
        .ok_or_else(|| RuntimeError::UnresolvableXdgPath("runtime_dir parent"))?;
    assert_dir(runtime_parent, 0o700, "runtime dir parent")?;

    // Config file: must exist, mode 0600.
    assert_file_mode(&config, 0o600, "config")?;

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
        runtime_dir: runtime,
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
fn assert_file_mode(path: &Path, expected_mode: u32, label: &str) -> Result<(), RuntimeError> {
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
fn assert_file_mode(path: &Path, _mode: u32, label: &str) -> Result<(), RuntimeError> {
    if !path.exists() {
        return Err(RuntimeError::PermissionInvariant {
            path: path.to_path_buf(),
            reason: format!("{label}: missing"),
        });
    }
    Ok(())
}
