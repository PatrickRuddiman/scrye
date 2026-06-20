//! Error types for the MCP surface.
//!
//! [`ConfigError`] covers startup/scoping configuration failures — most
//! importantly the mandatory `USER_EMAIL` env var (task 28). Per-tool
//! request failures map onto `rmcp`'s `ErrorData` (`McpError`) at the tool
//! boundary, not here.

use thiserror::Error;

/// Configuration / scoping errors raised before (or while) building the
/// scoped server state.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The mandatory `USER_EMAIL` scoping credential was unset or empty.
    /// The daemon fails fast on this — it must never serve unscoped.
    #[error("USER_EMAIL must be set to a non-empty email address")]
    UserEmailMissing,
}
