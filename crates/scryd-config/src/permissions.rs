use std::path::Path;

use crate::loader::ConfigError;

/// Verify the file's permission bits set no world (other) bits. v0.3.1
/// ships the config as `scryd:scryd 0640` so the daemon (uid scryd)
/// reads it and members of group scryd can read it; everyone else is
/// excluded. Anything with world bits set (0o644, 0o666, ...) is
/// rejected.
///
/// On non-Unix targets this is a no-op returning `Ok(())`; the spec is
/// Linux-only and Windows / macOS have no equivalent of the mode bits
/// to enforce.
#[cfg(unix)]
pub fn assert_no_world_bits(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::metadata(path)
        .map_err(|e| ConfigError::Read(path.to_path_buf(), e))?;
    let mode = meta.mode() & 0o777;
    if mode & 0o007 != 0 {
        return Err(ConfigError::PermissionInvariant {
            path: path.to_path_buf(),
            mode_seen: mode,
        });
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn assert_no_world_bits(_path: &Path) -> Result<(), ConfigError> {
    Ok(())
}
