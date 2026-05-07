//! Account row model + listing helper.

use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::StorageError;
use crate::handle::StorageHandle;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountRow {
    pub account_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub folders: Vec<String>,
    pub active: bool,
    pub mirrored_at: i64,
}

impl StorageHandle {
    /// Active accounts only, ordered by id. Inactivated rows are retained
    /// but not surfaced here.
    pub async fn list_active_accounts(&self) -> Result<Vec<AccountRow>, StorageError> {
        self.with_reader(|conn| {
            let mut stmt = conn.prepare(
                "SELECT account_id, host, port, username, folders_json, active, mirrored_at \
                 FROM accounts WHERE active = 1 ORDER BY account_id",
            )?;
            let rows = stmt.query_map([], row_to_account_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await
    }

    /// All accounts (active and inactivated). Used by reconcile and tests.
    pub async fn list_all_accounts(&self) -> Result<Vec<AccountRow>, StorageError> {
        self.with_reader(|conn| {
            let mut stmt = conn.prepare(
                "SELECT account_id, host, port, username, folders_json, active, mirrored_at \
                 FROM accounts ORDER BY account_id",
            )?;
            let rows = stmt.query_map([], row_to_account_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await
    }
}

pub(crate) fn row_to_account_row(row: &Row<'_>) -> rusqlite::Result<AccountRow> {
    let folders_json: String = row.get("folders_json")?;
    let folders: Vec<String> = serde_json::from_str(&folders_json).unwrap_or_default();
    Ok(AccountRow {
        account_id: row.get("account_id")?,
        host: row.get("host")?,
        port: row.get::<_, i64>("port")? as u16,
        username: row.get("username")?,
        folders,
        active: row.get::<_, i64>("active")? == 1,
        mirrored_at: row.get("mirrored_at")?,
    })
}

/// Used by reconcile and sync_state to resolve account-scoped row counts.
pub(crate) fn fetch_account(
    conn: &rusqlite::Connection,
    account_id: &str,
) -> Result<Option<AccountRow>, StorageError> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT account_id, host, port, username, folders_json, active, mirrored_at \
         FROM accounts WHERE account_id = ?1",
        params![account_id],
        row_to_account_row,
    )
    .optional()
    .map_err(StorageError::from)
}
