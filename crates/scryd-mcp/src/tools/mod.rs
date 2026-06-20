//! The six read MCP tools as scope-enforcing free functions. `server.rs` wraps
//! these in the thin `#[tool]` methods that serialize their typed DTOs into MCP
//! content.

pub mod read;
