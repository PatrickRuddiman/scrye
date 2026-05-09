//! Resolve `/etc/scryd/config.toml` and `/run/scryd/scryd.sock` for
//! the v0.2.0 sudo-installed daemon while keeping v0.1.0 per-user
//! XDG paths working unprivileged.

use std::path::{Path, PathBuf};

/// Trait the helpers use to read environment. Production calls
/// [`SystemEnv`] (delegates to `std::env::var`); tests inject
/// [`MapEnv`] with a fixed `HashMap`.
pub trait EnvSource {
    fn var(&self, name: &str) -> Option<String>;
}

/// Reads from the real process environment.
pub struct SystemEnv;

impl EnvSource for SystemEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// Test-only: a fixed map of env vars.
pub struct MapEnv(pub std::collections::HashMap<String, String>);

impl EnvSource for MapEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned()
    }
}

/// Resolve the config path the CLI should read / write.
///
/// Order:
///   1. `XDG_CONFIG_HOME/scryd/config.toml` (v0.1.0 per-user path)
///   2. `/etc/scryd/config.toml` (v0.2.0 sudo path; only when euid==0)
///   3. `HOME/.config/scryd/config.toml` (v0.1.0 fallback)
pub fn resolve_config_path_for(euid: u32, env: &dyn EnvSource) -> Result<PathBuf, String> {
    if let Some(home) = env.var("XDG_CONFIG_HOME") {
        if !home.is_empty() {
            return Ok(PathBuf::from(home).join("scryd").join("config.toml"));
        }
    }
    if euid == 0 {
        return Ok(PathBuf::from("/etc/scryd/config.toml"));
    }
    if let Some(home) = env.var("HOME") {
        if !home.is_empty() {
            return Ok(PathBuf::from(home)
                .join(".config")
                .join("scryd")
                .join("config.toml"));
        }
    }
    Err("XDG_CONFIG_HOME and HOME are both unset".to_string())
}

/// Resolve the daemon's UDS socket path.
///
/// Order:
///   1. `$XDG_RUNTIME_DIR/scryd/scryd.sock` (v0.1.0 per-user path)
///   2. `system_fallback` (v0.2.0; production passes `/run/scryd/scryd.sock`)
///
/// Returns `Err` with the system-fallback path embedded so the caller
/// can surface a "daemon not running" message naming the canonical
/// system path.
pub fn resolve_socket_path_for(
    env: &dyn EnvSource,
    system_fallback: &Path,
) -> Result<PathBuf, PathBuf> {
    if let Some(runtime) = env.var("XDG_RUNTIME_DIR") {
        if !runtime.is_empty() {
            let p = PathBuf::from(runtime).join("scryd").join("scryd.sock");
            if p.exists() {
                return Ok(p);
            }
        }
    }
    if system_fallback.exists() {
        return Ok(system_fallback.to_path_buf());
    }
    Err(system_fallback.to_path_buf())
}

/// Path to the v0.2.0 system socket. Used as the default
/// `system_fallback` argument by [`resolve_socket_path_for`] callers.
pub const SYSTEM_SOCKET_PATH: &str = "/run/scryd/scryd.sock";
