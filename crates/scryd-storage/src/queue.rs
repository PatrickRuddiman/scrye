//! `index_queue` helpers. The search-engine drainer (task 10) consumes these.

use rusqlite::{params, OptionalExtension, Row};

use crate::db::StorageError;
use crate::handle::StorageHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexQueueRow {
    pub message_id: String,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub queued_at: i64,
    pub failed_permanent: bool,
}

/// Aggregate health of `index_queue`, surfaced through `GET /status` so a
/// crash loop or a wave of permanent failures is visible without inspecting
/// the journal or the database by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueHealth {
    /// Rows still drainable (not permanently failed).
    pub depth: i64,
    /// Rows that hit the permanent-failure ceiling.
    pub failed_permanent: i64,
    /// The most recent per-item failure, if any.
    pub last_error: Option<QueueLastError>,
}

/// The most recent failed `index_queue` row. `error` is already sanitized by
/// the drainer before storage, so it is safe to echo to operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueLastError {
    pub message_id: String,
    pub error: String,
    pub attempts: u32,
}

impl StorageHandle {
    /// Enqueue a message for indexing. No-op if already queued (or already
    /// drained successfully — the row would be absent in that case).
    pub async fn enqueue(&self, message_id: &str) -> Result<(), StorageError> {
        let id = message_id.to_string();
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO index_queue \
                    (message_id, attempts, last_error, queued_at, failed_permanent) \
                 VALUES (?1, 0, NULL, strftime('%s','now'), 0)",
                params![id],
            )?;
            Ok(())
        })
        .await
    }

    /// Fetch up to `limit` rows from the queue, oldest first, skipping any
    /// that have hit the permanent-failure ceiling.
    pub async fn pop_batch(&self, limit: usize) -> Result<Vec<IndexQueueRow>, StorageError> {
        let limit = limit as i64;
        self.with_reader(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT message_id, attempts, last_error, queued_at, failed_permanent \
                 FROM index_queue \
                 WHERE failed_permanent = 0 \
                 ORDER BY queued_at ASC \
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], row_to_queue_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await
    }

    /// Remove a queue row after a successful index submission.
    pub async fn delete_queue_row(&self, message_id: &str) -> Result<(), StorageError> {
        let id = message_id.to_string();
        self.with_writer(move |conn| {
            conn.execute(
                "DELETE FROM index_queue WHERE message_id = ?1",
                params![id],
            )?;
            Ok(())
        })
        .await
    }

    /// Record a failed attempt. Returns `true` if the row crossed the
    /// `max_attempts` threshold and is now flagged `failed_permanent`.
    pub async fn mark_failed(
        &self,
        message_id: &str,
        err: &str,
        max_attempts: u32,
    ) -> Result<bool, StorageError> {
        let id = message_id.to_string();
        let err = err.to_string();
        self.with_writer(move |conn| {
            let current: Option<i64> = conn
                .query_row(
                    "SELECT attempts FROM index_queue WHERE message_id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(prev_attempts) = current else {
                return Ok(false);
            };
            let new_attempts = prev_attempts + 1;
            let permanent = new_attempts >= (max_attempts as i64);
            conn.execute(
                "UPDATE index_queue SET \
                    attempts = ?1, \
                    last_error = ?2, \
                    failed_permanent = ?3, \
                    last_failed_at = strftime('%s','now') \
                 WHERE message_id = ?4",
                params![new_attempts, err, permanent as i64, id],
            )?;
            Ok(permanent)
        })
        .await
    }

    /// Wipe the queue and re-enqueue every non-tombstoned message in one
    /// statement. Used by the api slice's `/internal/reindex` flow.
    pub async fn reenqueue_all_messages(&self) -> Result<u64, StorageError> {
        self.with_writer(|conn| {
            let n = conn.execute(
                "INSERT OR REPLACE INTO index_queue \
                    (message_id, attempts, last_error, queued_at, failed_permanent) \
                 SELECT message_id, 0, NULL, strftime('%s','now'), 0 \
                 FROM messages WHERE tombstoned_at IS NULL",
                [],
            )?;
            Ok(n as u64)
        })
        .await
    }

    /// Test/inspection helper: list all queue rows including permanently-
    /// failed ones, oldest first.
    pub async fn list_queue(&self) -> Result<Vec<IndexQueueRow>, StorageError> {
        self.with_reader(|conn| {
            let mut stmt = conn.prepare(
                "SELECT message_id, attempts, last_error, queued_at, failed_permanent \
                 FROM index_queue ORDER BY queued_at ASC",
            )?;
            let rows = stmt.query_map([], row_to_queue_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await
    }

    /// Aggregate queue health for `GET /status`: drainable depth, the count of
    /// permanently-failed rows, and the single most recent failure (by
    /// `last_failed_at`). Read-only.
    pub async fn queue_health(&self) -> Result<QueueHealth, StorageError> {
        self.with_reader(|conn| {
            let depth: i64 = conn.query_row(
                "SELECT COUNT(*) FROM index_queue WHERE failed_permanent = 0",
                [],
                |r| r.get(0),
            )?;
            let failed_permanent: i64 = conn.query_row(
                "SELECT COUNT(*) FROM index_queue WHERE failed_permanent = 1",
                [],
                |r| r.get(0),
            )?;
            let last_error = conn
                .query_row(
                    "SELECT message_id, last_error, attempts FROM index_queue \
                     WHERE last_error IS NOT NULL \
                     ORDER BY last_failed_at DESC, queued_at DESC LIMIT 1",
                    [],
                    |r| {
                        Ok(QueueLastError {
                            message_id: r.get(0)?,
                            error: r.get::<_, String>(1)?,
                            attempts: r.get::<_, i64>(2)? as u32,
                        })
                    },
                )
                .optional()?;
            Ok(QueueHealth {
                depth,
                failed_permanent,
                last_error,
            })
        })
        .await
    }
}

fn row_to_queue_row(row: &Row<'_>) -> rusqlite::Result<IndexQueueRow> {
    Ok(IndexQueueRow {
        message_id: row.get("message_id")?,
        attempts: row.get::<_, i64>("attempts")? as u32,
        last_error: row.get("last_error")?,
        queued_at: row.get("queued_at")?,
        failed_permanent: row.get::<_, i64>("failed_permanent")? != 0,
    })
}
