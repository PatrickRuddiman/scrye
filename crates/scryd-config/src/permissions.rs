use std::path::Path;

use crate::loader::ConfigError;

/// Verify the file's permission bits are exactly `0600`.
///
/// On non-Unix targets this is a no-op returning `Ok(())`; the spec is
/// Linux-only and Windows / macOS have no equivalent of `0600` to enforce.
#[cfg(unix)]
pub fn assert_mode_0600(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::metadata(path)
        .map_err(|e| ConfigError::Read(path.to_path_buf(), e))?;
    let mode = meta.mode() & 0o777;
    if mode != 0o600 {
        return Err(ConfigError::PermissionTooOpen(mode));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn assert_mode_0600(_path: &Path) -> Result<(), ConfigError> {
    Ok(())
}
