//! Typed reader for `config.toml`.
//!
//! Layout matches the cli slice's documented schema. The IMAP credential lives
//! in [`AccountPassword`], a wrapper that does not implement [`std::fmt::Display`]
//! and prints `[REDACTED]` from its [`std::fmt::Debug`] impl, so the secret can
//! never be accidentally interpolated into a log line.

mod loader;
mod permissions;
mod secret;

pub use loader::{Config, ConfigError, IndexersCfg, ServerCfg, SyncCfg};
pub use loader::AccountCfg;
pub use permissions::assert_no_world_bits;
pub use secret::AccountPassword;
