//! XDG Base Directory resolution. The multi-instance-isolation slice pins
//! a strict per-user layout; this module resolves it from the environment.

use std::path::PathBuf;

use crate::RuntimeError;

const SCRYD_DIR: &str = "scryd";
const ASSETS_SUBDIR: &str = "assets";
const META_FILENAME: &str = "config.toml";

/// `$XDG_RUNTIME_DIR/scryd`, OR `$XDG_RUNTIME_DIR` itself if it
/// already ends in `/scryd` (the systemd unit sets it directly to
/// `/run/scryd`). The env var is mandatory — without it, scryd has
/// no per-user-and-volatile directory to put the API socket in.
pub fn runtime_dir() -> Result<PathBuf, RuntimeError> {
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(s) if !s.is_empty() => {
            let p = PathBuf::from(&s);
            if p.file_name().and_then(|n| n.to_str()) == Some(SCRYD_DIR) {
                Ok(p)
            } else {
                Ok(p.join(SCRYD_DIR))
            }
        }
        _ => Err(RuntimeError::MissingRuntimeDir),
    }
}

/// `$XDG_CONFIG_HOME/scryd/config.toml`, falling back to
/// `$HOME/.config/scryd/config.toml`. The systemd unit sets
/// `XDG_CONFIG_HOME=/etc/scryd` directly; we detect that and skip
/// the redundant `/scryd` suffix.
pub fn config_path() -> Result<PathBuf, RuntimeError> {
    if let Some(base) = read_env("XDG_CONFIG_HOME") {
        let dir = if base.file_name().and_then(|n| n.to_str()) == Some(SCRYD_DIR) {
            base
        } else {
            base.join(SCRYD_DIR)
        };
        return Ok(dir.join(META_FILENAME));
    }
    if let Some(home) = read_env("HOME") {
        return Ok(home.join(".config").join(SCRYD_DIR).join(META_FILENAME));
    }
    Err(RuntimeError::UnresolvableXdgPath("config_path"))
}

/// `$XDG_DATA_HOME/scryd`, falling back to `$HOME/.local/share/scryd`.
/// The systemd unit sets `XDG_DATA_HOME=/var/lib/scryd` directly;
/// we detect that and skip the redundant `/scryd` suffix.
pub fn data_dir() -> Result<PathBuf, RuntimeError> {
    if let Some(base) = read_env("XDG_DATA_HOME") {
        if base.file_name().and_then(|n| n.to_str()) == Some(SCRYD_DIR) {
            return Ok(base);
        }
        return Ok(base.join(SCRYD_DIR));
    }
    if let Some(home) = read_env("HOME") {
        return Ok(home.join(".local").join("share").join(SCRYD_DIR));
    }
    Err(RuntimeError::UnresolvableXdgPath("data_dir"))
}

/// `<data_dir>/assets` — where the install script puts the T5 weights.
pub fn assets_dir() -> Result<PathBuf, RuntimeError> {
    Ok(data_dir()?.join(ASSETS_SUBDIR))
}

fn read_env(key: &str) -> Option<PathBuf> {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}
