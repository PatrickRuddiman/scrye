//! `scryd-mcp` — the Model Context Protocol surface for a running scryd
//! daemon.
//!
//! Exposes scryd's six read/search operations as MCP **tools** over Streamable
//! HTTP on a loopback TCP listener, scoped to the account(s) owned by the
//! mandatory `USER_EMAIL` env var. Reuses the daemon's storage/search logic;
//! only the transport and the `USER_EMAIL` scoping boundary are new. Fetching
//! and indexing run automatically inside the daemon — MCP is read-only, the
//! sole external surface.

pub mod dto;
pub mod error;
pub mod scope;
pub mod serve;
pub mod server;
pub mod state;
pub mod tools;

pub use error::ConfigError;
pub use scope::AccountScope;
pub use serve::{serve, serve_listener};
pub use server::McpServer;
pub use state::{DaemonHealthSnapshot, McpState};
