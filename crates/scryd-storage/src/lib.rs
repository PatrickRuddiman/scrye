//! `meta.sqlite` schema, migrations, and connection bootstrap.
//!
//! Tasks 04–06 layer typed read/write helpers, the raw `.eml` file store,
//! tombstones, and queue helpers on top of the schema this module pins.

pub mod db;
pub mod migrations;

pub use db::{open, open_read_only, StorageError};
pub use migrations::MigrationError;
