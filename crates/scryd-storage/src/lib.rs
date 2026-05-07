//! `meta.sqlite` schema, migrations, connection topology, and typed
//! read/write helpers.

pub mod accounts;
pub mod db;
pub mod handle;
pub mod messages;
pub mod migrations;
pub mod raw;
pub mod reconcile;
pub mod sync_state;
pub mod threading;

pub use accounts::AccountRow;
pub use db::{open, open_read_only, StorageError};
pub use handle::StorageHandle;
pub use messages::{Address, MessageInsert, MessageRow};
pub use migrations::MigrationError;
pub use reconcile::ReconcileDiff;
pub use sync_state::{AccountHealth, SyncStateRow, SyncStateUpdate};
