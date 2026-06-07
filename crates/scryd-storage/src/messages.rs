//! Typed message rows and idempotent insert helpers.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::StorageError;
use crate::handle::StorageHandle;
use crate::threading;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Address {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MessageInsert {
    pub message_id: String,
    pub account_id: String,
    pub folder: String,
    pub server_uid: u32,
    pub uidvalidity: u32,
    pub header_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub sender_addr: String,
    pub sender_name: Option<String>,
    pub recipients_to: Vec<Address>,
    pub recipients_cc: Vec<Address>,
    pub subject: Option<String>,
    pub date_unix: i64,
    pub raw_path: String,
    pub body_md: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub message_id: String,
    pub account_id: String,
    pub folder: String,
    pub server_uid: u32,
    pub uidvalidity: u32,
    pub header_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub thread_id: String,
    pub sender_addr: String,
    pub sender_name: Option<String>,
    pub recipients_to: Vec<Address>,
    pub recipients_cc: Vec<Address>,
    pub subject: Option<String>,
    pub date_unix: i64,
    pub raw_path: String,
    pub body_md: String,
    pub size_bytes: u64,
    pub tombstoned_at: Option<i64>,
}

const SELECT_COLUMNS: &str = "\
    message_id, account_id, folder, server_uid, uidvalidity, \
    header_message_id, in_reply_to, references_json, thread_id, \
    sender_addr, sender_name, recipients_to_json, recipients_cc_json, \
    subject, date_unix, raw_path, body_md, size_bytes, tombstoned_at";

impl StorageHandle {
    /// Idempotent insert of a message. If a row with the same
    /// `(account_id, folder, server_uid, uidvalidity)` already exists, only
    /// the body / size / raw-path fields are refreshed; thread_id and the
    /// account-scoped identity are preserved.
    ///
    /// Returns `true` when the message is new or its `body_md` changed,
    /// meaning the caller should enqueue the message for (re-)indexing.
    /// Returns `false` when the stored content is identical, so the sink can
    /// skip an unnecessary re-enqueue during normal IMAP sync.
    pub async fn insert_message(&self, m: MessageInsert) -> Result<bool, StorageError> {
        self.with_writer(move |conn| {
            let thread_id = threading::resolve_thread_id(
                conn,
                &m.account_id,
                m.in_reply_to.as_deref(),
                &m.references,
            )?;

            let references_json = serde_json::to_string(&m.references)?;
            let to_json = serde_json::to_string(&m.recipients_to)?;
            let cc_json = serde_json::to_string(&m.recipients_cc)?;

            // Check whether a row with identical coordinates AND identical
            // body already exists.  If so, no re-indexing is needed.
            let already_unchanged: bool = conn
                .query_row(
                    "SELECT 1 FROM messages \
                     WHERE account_id = ?1 AND folder = ?2 \
                       AND server_uid = ?3 AND uidvalidity = ?4 \
                       AND body_md = ?5",
                    params![
                        m.account_id,
                        m.folder,
                        m.server_uid as i64,
                        m.uidvalidity as i64,
                        m.body_md,
                    ],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();

            conn.execute(
                "INSERT INTO messages (\
                    message_id, account_id, folder, server_uid, uidvalidity, \
                    header_message_id, in_reply_to, references_json, thread_id, \
                    sender_addr, sender_name, recipients_to_json, recipients_cc_json, \
                    subject, date_unix, raw_path, body_md, size_bytes\
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18) \
                ON CONFLICT(account_id, folder, server_uid, uidvalidity) DO UPDATE SET \
                    body_md = excluded.body_md, \
                    size_bytes = excluded.size_bytes, \
                    raw_path = excluded.raw_path",
                params![
                    m.message_id,
                    m.account_id,
                    m.folder,
                    m.server_uid,
                    m.uidvalidity,
                    m.header_message_id,
                    m.in_reply_to,
                    references_json,
                    thread_id,
                    m.sender_addr,
                    m.sender_name,
                    to_json,
                    cc_json,
                    m.subject,
                    m.date_unix,
                    m.raw_path,
                    m.body_md,
                    m.size_bytes as i64,
                ],
            )?;
            Ok(!already_unchanged)
        })
        .await
    }

    /// Fetch a single message by its scryd-internal id. Returns `None` if
    /// no row exists; tombstoned rows are still returned (callers filter).
    pub async fn get_message(&self, id: &str) -> Result<Option<MessageRow>, StorageError> {
        let id = id.to_string();
        self.with_reader(move |conn| {
            let sql = format!(
                "SELECT {SELECT_COLUMNS} FROM messages WHERE message_id = ?1"
            );
            conn.query_row(&sql, params![id], row_to_message_row)
                .optional()
                .map_err(StorageError::from)
        })
        .await
    }

    /// Fetch every non-tombstoned message in a thread, oldest first.
    pub async fn get_thread(&self, thread_id: &str) -> Result<Vec<MessageRow>, StorageError> {
        let thread_id = thread_id.to_string();
        self.with_reader(move |conn| {
            let sql = format!(
                "SELECT {SELECT_COLUMNS} FROM messages \
                 WHERE thread_id = ?1 AND tombstoned_at IS NULL \
                 ORDER BY date_unix ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![thread_id], row_to_message_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await
    }

    /// Mark a message tombstoned (server-side disappeared). Idempotent.
    pub async fn tombstone(&self, message_id: &str) -> Result<(), StorageError> {
        let id = message_id.to_string();
        self.with_writer(move |conn| {
            conn.execute(
                "UPDATE messages SET tombstoned_at = strftime('%s','now') \
                 WHERE message_id = ?1 AND tombstoned_at IS NULL",
                params![id],
            )?;
            Ok(())
        })
        .await
    }

    /// Return the server-side UID set scryd has indexed under
    /// `(account_id, folder, uidvalidity)`. The supervisor's
    /// tombstone scan diffs this against `UID SEARCH ALL` to
    /// identify locally-present-but-server-absent rows.
    pub async fn list_uids_for(
        &self,
        account_id: &str,
        folder: &str,
        uidvalidity: u32,
    ) -> Result<Vec<u32>, StorageError> {
        let account_id = account_id.to_string();
        let folder = folder.to_string();
        self.with_reader(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT server_uid FROM messages \
                 WHERE account_id = ?1 AND folder = ?2 AND uidvalidity = ?3 \
                   AND tombstoned_at IS NULL",
            )?;
            let rows = stmt
                .query_map(
                    params![account_id, folder, uidvalidity as i64],
                    |row| row.get::<_, i64>(0).map(|v| v as u32),
                )?
                .collect::<rusqlite::Result<Vec<u32>>>()?;
            Ok(rows)
        })
        .await
    }
}

fn row_to_message_row(row: &Row<'_>) -> rusqlite::Result<MessageRow> {
    let refs_json: Option<String> = row.get("references_json")?;
    let references = refs_json
        .map(|s| serde_json::from_str(&s).unwrap_or_default())
        .unwrap_or_default();
    let to_json: Option<String> = row.get("recipients_to_json")?;
    let recipients_to = to_json
        .map(|s| serde_json::from_str(&s).unwrap_or_default())
        .unwrap_or_default();
    let cc_json: Option<String> = row.get("recipients_cc_json")?;
    let recipients_cc = cc_json
        .map(|s| serde_json::from_str(&s).unwrap_or_default())
        .unwrap_or_default();

    Ok(MessageRow {
        message_id: row.get("message_id")?,
        account_id: row.get("account_id")?,
        folder: row.get("folder")?,
        server_uid: row.get("server_uid")?,
        uidvalidity: row.get("uidvalidity")?,
        header_message_id: row.get("header_message_id")?,
        in_reply_to: row.get("in_reply_to")?,
        references,
        thread_id: row.get("thread_id")?,
        sender_addr: row.get("sender_addr")?,
        sender_name: row.get("sender_name")?,
        recipients_to,
        recipients_cc,
        subject: row.get("subject")?,
        date_unix: row.get("date_unix")?,
        raw_path: row.get("raw_path")?,
        body_md: row.get("body_md")?,
        size_bytes: row.get::<_, i64>("size_bytes")? as u64,
        tombstoned_at: row.get("tombstoned_at")?,
    })
}
