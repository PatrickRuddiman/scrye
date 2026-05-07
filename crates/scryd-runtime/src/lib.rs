//! Daemon runtime: XDG resolution, preflight self-checks, the
//! `StorageMessageSink` that adapts scryd-imap → scryd-mime → scryd-storage,
//! and the `serve()` orchestration entry that the `scryd serve` CLI verb
//! invokes.
//!
//! The full live orchestration (binding the imap-sync scheduler + indexer
//! drainer + api router into a tokio runtime) is partially deferred along
//! with the IMAP and witchcraft live integrations; this crate ships every
//! testable seam (XDG paths, preflight invariants, sink wiring).

pub mod preflight;
pub mod sink;
pub mod xdg;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("XDG_RUNTIME_DIR is unset; scryd needs a per-user runtime dir")]
    MissingRuntimeDir,
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
pub use sink::StorageMessageSink;
pub use xdg::{assets_dir, config_path, data_dir, runtime_dir};
