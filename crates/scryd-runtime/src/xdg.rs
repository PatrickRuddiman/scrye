//! XDG Base Directory resolution. The multi-instance-isolation slice pins
//! a strict per-user layout; this module resolves it from the environment.

use std::path::PathBuf;

use crate::RuntimeError;

const SCRYD_DIR: &str = "scryd";
const ASSETS_SUBDIR: &str = "assets";
const META_FILENAME: &str = "config.toml";

/// `$XDG_RUNTIME_DIR/scryd`. The env var is mandatory — without it, scryd
/// has no per-user-and-volatile directory to put the API socket in.
pub fn runtime_dir() -> Result<PathBuf, RuntimeError> {
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(s) if !s.is_empty() => Ok(PathBuf::from(s).join(SCRYD_DIR)),
        _ => Err(RuntimeError::MissingRuntimeDir),
    }
}

/// `$XDG_CONFIG_HOME/scryd/config.toml`, falling back to
/// `$HOME/.config/scryd/config.toml`.
pub fn config_path() -> Result<PathBuf, RuntimeError> {
    if let Some(base) = read_env("XDG_CONFIG_HOME") {
        return Ok(base.join(SCRYD_DIR).join(META_FILENAME));
    }
    if let Some(home) = read_env("HOME") {
        return Ok(home.join(".config").join(SCRYD_DIR).join(META_FILENAME));
    }
    Err(RuntimeError::UnresolvableXdgPath("config_path"))
}

/// `$XDG_DATA_HOME/scryd`, falling back to `$HOME/.local/share/scryd`.
pub fn data_dir() -> Result<PathBuf, RuntimeError> {
    if let Some(base) = read_env("XDG_DATA_HOME") {
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
