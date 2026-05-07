//! Daemon-wide tracing initialization plus the macros every other crate uses
//! to emit categorized failure events and lifecycle events.
//!
//! On stderr-is-TTY (developer running `cargo run`), the subscriber renders
//! human-readable lines. Otherwise (the systemd-managed daemon), it writes
//! JSON-Lines so journald captures structured fields.

pub mod categories;
mod macros;

use std::sync::atomic::{AtomicBool, Ordering};

use tracing_subscriber::EnvFilter;

pub use categories::{category, kind};

/// Default tracing-env filter applied when `RUST_LOG` is unset.
pub const DEFAULT_FILTER: &str = "scryd=info,scryd_api=info,scryd_imap=info,scryd_search=info,scryd_storage=info,scryd_mime=info,scryd_runtime=info,warn";

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("scryd-log already initialized in this process")]
    AlreadyInitialized,
    #[error("a tracing global subscriber was already set by another crate")]
    SubscriberAlreadySet,
}

static INIT: AtomicBool = AtomicBool::new(false);

/// Install the daemon's global tracing subscriber. Idempotent: a second call
/// returns [`InitError::AlreadyInitialized`].
pub fn init() -> Result<(), InitError> {
    if INIT.swap(true, Ordering::SeqCst) {
        return Err(InitError::AlreadyInitialized);
    }

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(true);

    let result = if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        tracing::subscriber::set_global_default(builder.pretty().finish())
    } else {
        tracing::subscriber::set_global_default(
            builder.json().flatten_event(true).with_current_span(false).with_span_list(false).finish(),
        )
    };

    result.map_err(|_| InitError::SubscriberAlreadySet)
}
