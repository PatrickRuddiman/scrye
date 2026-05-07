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
                    failed_permanent = ?3 \
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
