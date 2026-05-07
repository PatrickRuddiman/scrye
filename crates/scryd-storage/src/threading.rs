//! JWZ-ish thread resolution: walk References + In-Reply-To, return any
//! existing same-account row's `thread_id` whose `header_message_id` matches.
//! Otherwise mint a new UUIDv4.

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::StorageError;

/// Resolve a thread id for a message about to be inserted. Caller must hold
/// the write lock (or a transaction) so the SELECT and the eventual INSERT
/// happen atomically.
pub fn resolve_thread_id(
    conn: &Connection,
    account_id: &str,
    in_reply_to: Option<&str>,
    references: &[String],
) -> Result<String, StorageError> {
    // Walk newest reference first (the immediate parent), then older
    // ancestors via References, then In-Reply-To as a fallback. The first
    // match wins.
    let mut candidates: Vec<&str> = references.iter().map(String::as_str).rev().collect();
    if let Some(irt) = in_reply_to {
        candidates.push(irt);
    }

    for cand in candidates {
        let found: Option<String> = conn
            .query_row(
                "SELECT thread_id FROM messages \
                 WHERE account_id = ?1 AND header_message_id = ?2 \
                 LIMIT 1",
                params![account_id, cand],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        if let Some(tid) = found {
            return Ok(tid);
        }
    }

    Ok(uuid::Uuid::new_v4().to_string())
}
