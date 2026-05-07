//! Per-(account, folder) sync state row plus typed merge updates.

use std::str::FromStr;

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::StorageError;
use crate::handle::StorageHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountHealth {
    Unknown,
    Active,
    Degraded,
    AuthRejected,
    QuotaExceeded,
    Unreachable,
    TlsFailed,
}

impl AccountHealth {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Active => "active",
            Self::Degraded => "degraded",
            Self::AuthRejected => "auth-rejected",
            Self::QuotaExceeded => "quota-exceeded",
            Self::Unreachable => "unreachable",
            Self::TlsFailed => "tls-failed",
        }
    }
}

impl std::fmt::Display for AccountHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AccountHealth {
    type Err = StorageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "unknown" => Self::Unknown,
            "active" => Self::Active,
            "degraded" => Self::Degraded,
            "auth-rejected" => Self::AuthRejected,
            "quota-exceeded" => Self::QuotaExceeded,
            "unreachable" => Self::Unreachable,
            "tls-failed" => Self::TlsFailed,
            other => return Err(StorageError::InvalidHealth(other.to_string())),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyncStateRow {
    pub account_id: String,
    pub folder: String,
    pub uidvalidity: Option<u32>,
    pub last_seen_uid: u32,
    pub last_full_sync_at: Option<i64>,
    pub last_idle_at: Option<i64>,
    pub last_error: Option<String>,
    pub account_health: AccountHealth,
    pub backoff_until: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct SyncStateUpdate {
    pub uidvalidity: Option<u32>,
    pub last_seen_uid: Option<u32>,
    pub last_full_sync_at: Option<i64>,
    pub last_idle_at: Option<i64>,
    pub last_error: Option<Option<String>>,
    pub account_health: Option<AccountHealth>,
    pub backoff_until: Option<Option<i64>>,
}

impl StorageHandle {
    pub async fn get_sync_state(
        &self,
        account_id: &str,
        folder: &str,
    ) -> Result<Option<SyncStateRow>, StorageError> {
        let account_id = account_id.to_string();
        let folder = folder.to_string();
        self.with_reader(move |conn| {
            conn.query_row(
                "SELECT account_id, folder, uidvalidity, last_seen_uid, \
                        last_full_sync_at, last_idle_at, last_error, \
                        account_health, backoff_until \
                 FROM sync_state WHERE account_id = ?1 AND folder = ?2",
                params![account_id, folder],
                row_to_sync_state_row,
            )
            .optional()
            .map_err(StorageError::from)
        })
        .await
    }

    /// Insert or update the sync state row. Only fields explicitly set on
    /// `update` are written; everything else is preserved.
    pub async fn update_sync_state(
        &self,
        account_id: &str,
        folder: &str,
        update: SyncStateUpdate,
    ) -> Result<(), StorageError> {
        let account_id = account_id.to_string();
        let folder = folder.to_string();
        self.with_writer(move |conn| {
            // Ensure a row exists.
            conn.execute(
                "INSERT INTO sync_state (account_id, folder) VALUES (?1, ?2) \
                 ON CONFLICT(account_id, folder) DO NOTHING",
                params![account_id, folder],
            )?;

            // Apply each Some field as a separate UPDATE — simple and
            // explicit; v1 doesn't try to batch into one dynamic statement.
            if let Some(v) = update.uidvalidity {
                conn.execute(
                    "UPDATE sync_state SET uidvalidity = ?1 WHERE account_id = ?2 AND folder = ?3",
                    params![v, account_id, folder],
                )?;
            }
            if let Some(v) = update.last_seen_uid {
                conn.execute(
                    "UPDATE sync_state SET last_seen_uid = ?1 WHERE account_id = ?2 AND folder = ?3",
                    params![v, account_id, folder],
                )?;
            }
            if let Some(v) = update.last_full_sync_at {
                conn.execute(
                    "UPDATE sync_state SET last_full_sync_at = ?1 WHERE account_id = ?2 AND folder = ?3",
                    params![v, account_id, folder],
                )?;
            }
            if let Some(v) = update.last_idle_at {
                conn.execute(
                    "UPDATE sync_state SET last_idle_at = ?1 WHERE account_id = ?2 AND folder = ?3",
                    params![v, account_id, folder],
                )?;
            }
            if let Some(v) = update.last_error {
                conn.execute(
                    "UPDATE sync_state SET last_error = ?1 WHERE account_id = ?2 AND folder = ?3",
                    params![v, account_id, folder],
                )?;
            }
            if let Some(v) = update.account_health {
                conn.execute(
                    "UPDATE sync_state SET account_health = ?1 WHERE account_id = ?2 AND folder = ?3",
                    params![v.as_str(), account_id, folder],
                )?;
            }
            if let Some(v) = update.backoff_until {
                conn.execute(
                    "UPDATE sync_state SET backoff_until = ?1 WHERE account_id = ?2 AND folder = ?3",
                    params![v, account_id, folder],
                )?;
            }
            Ok(())
        })
        .await
    }
}

fn row_to_sync_state_row(row: &Row<'_>) -> rusqlite::Result<SyncStateRow> {
    let health_str: String = row.get("account_health")?;
    let health = AccountHealth::from_str(&health_str).unwrap_or(AccountHealth::Unknown);
    Ok(SyncStateRow {
        account_id: row.get("account_id")?,
        folder: row.get("folder")?,
        uidvalidity: row.get::<_, Option<i64>>("uidvalidity")?.map(|v| v as u32),
        last_seen_uid: row.get::<_, i64>("last_seen_uid")? as u32,
        last_full_sync_at: row.get("last_full_sync_at")?,
        last_idle_at: row.get("last_idle_at")?,
        last_error: row.get("last_error")?,
        account_health: health,
        backoff_until: row.get("backoff_until")?,
    })
}
