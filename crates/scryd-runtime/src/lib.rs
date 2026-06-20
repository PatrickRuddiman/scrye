//! Daemon runtime: XDG resolution, preflight self-checks, the
//! `StorageMessageSink` that adapts scryd-imap → scryd-mime →
//! scryd-storage, and the `serve()` orchestration entry that the
//! `scryd` daemon binary invokes.
//!
//! `serve()` wires logging + preflight + storage + WitchcraftIndexer
//! + drainer + scheduler + the scryd-mcp Streamable-HTTP server into a
//! single async entry point. Every component shuts down cooperatively
//! on SIGTERM/SIGINT.

pub mod preflight;
pub mod serve;
pub mod sink;
pub mod xdg;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("USER_EMAIL must be set to a non-empty email address; the daemon refuses to run unscoped")]
    UserEmail,
    #[error("HOME and XDG_*_HOME are both unset; cannot resolve {0}")]
    UnresolvableXdgPath(&'static str),
    #[error("permission invariant violated for {path}: {reason}")]
    PermissionInvariant {
        path: std::path::PathBuf,
        reason: String,
    },
    #[error("filesystem: {0}")]
    Io(#[from] std::io::Error),
    #[error("config error: {0}")]
    Config(#[from] scryd_config::ConfigError),
    #[error("storage error: {0}")]
    Storage(#[from] scryd_storage::StorageError),
}

pub use preflight::{run as run_preflight, PreflightOk};
pub use serve::{serve, serve_init, serve_run, ServeContext};
pub use sink::StorageMessageSink;
pub use xdg::{assets_dir, config_path, data_dir};
