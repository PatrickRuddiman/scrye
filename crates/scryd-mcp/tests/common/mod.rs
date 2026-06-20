//! Shared test seeding: a temp `StorageHandle` with one **owned** account
//! (username matches `OWNER_EMAIL` case-insensitively) and one **foreign**
//! account, each with one indexed message, plus an `InMemoryIndexer`. Tests use
//! this to prove every tool refuses foreign-account data.
#![allow(dead_code)]

use std::sync::Arc;

use scryd_mcp::{AccountScope, McpState};
use scryd_search::{IndexSubmit, Indexer, InMemoryIndexer, MessageId};
use scryd_storage::{Address, MessageInsert, StorageHandle};
use tempfile::TempDir;

pub const OWNER_EMAIL: &str = "owner@example.com";
/// Stored mixed-case to exercise case-insensitive `username` matching.
pub const OWNED_USERNAME: &str = "Owner@Example.com";
pub const FOREIGN_USERNAME: &str = "someone-else@example.com";
pub const OWNED_ID: &str = "owned";
pub const FOREIGN_ID: &str = "foreign";

pub const OWNED_MSG: &str = "owned:a@example.com";
pub const FOREIGN_MSG: &str = "foreign:b@example.com";

/// A common search term present in both messages' indexed documents, so a
/// query matches both at the index layer and only scope can exclude the
/// foreign hit.
pub const COMMON_TERM: &str = "report";

pub struct Seed {
    pub dir: TempDir,
    pub storage: StorageHandle,
    pub indexer: Arc<InMemoryIndexer>,
    pub owned_thread: String,
    pub foreign_thread: String,
}

impl Seed {
    pub fn scope() -> AccountScope {
        AccountScope::new(OWNER_EMAIL).unwrap()
    }

    pub fn state(&self) -> McpState {
        McpState::new(self.storage.clone(), self.indexer.clone(), Self::scope())
    }

    /// Build a state whose scope is bound to an arbitrary email — used to
    /// exercise the empty-scope ("no owned account") fallback.
    pub fn state_with_scope(&self, email: &str) -> McpState {
        McpState::new(
            self.storage.clone(),
            self.indexer.clone(),
            AccountScope::new(email).unwrap(),
        )
    }
}

async fn insert_account(storage: &StorageHandle, id: &str, username: &str) {
    // Test inputs are fixed constants; inline the SQL to avoid taking a direct
    // `rusqlite` dependency just for the bind params.
    let sql = format!(
        "INSERT INTO accounts (account_id, host, port, username, folders_json, active, mirrored_at) \
         VALUES ('{id}', 'imap.example.com', 993, '{username}', '[\"INBOX\"]', 1, 0)"
    );
    storage
        .with_writer(move |conn| {
            conn.execute(&sql, [])?;
            Ok(())
        })
        .await
        .unwrap();
}

async fn insert_message(
    data_dir: &std::path::Path,
    storage: &StorageHandle,
    indexer: &InMemoryIndexer,
    message_id: &str,
    account_id: &str,
    subject: &str,
    body: &str,
    date_unix: i64,
) -> String {
    let raw = format!(
        "From: Alice <alice@example.com>\r\nSubject: {subject}\r\nDate: Thu, 30 Apr 2026 12:00:00 +0000\r\n\r\n{body}\r\n"
    );
    let raw_path = scryd_storage::raw::write_raw(
        data_dir,
        account_id,
        message_id,
        date_unix,
        raw.as_bytes(),
    )
    .await
    .unwrap();

    storage
        .insert_message(MessageInsert {
            message_id: message_id.to_string(),
            account_id: account_id.to_string(),
            folder: "INBOX".to_string(),
            server_uid: 1,
            uidvalidity: 1,
            header_message_id: Some(message_id.to_string()),
            in_reply_to: None,
            references: vec![],
            sender_addr: "alice@example.com".to_string(),
            sender_name: Some("Alice".to_string()),
            recipients_to: vec![Address {
                addr: "bob@example.com".to_string(),
                name: None,
            }],
            recipients_cc: vec![],
            subject: Some(subject.to_string()),
            date_unix,
            raw_path: raw_path.to_string_lossy().into_owned(),
            body_md: body.to_string(),
            size_bytes: raw.len() as u64,
        })
        .await
        .unwrap();

    indexer
        .submit(IndexSubmit {
            message_id: MessageId::new(message_id),
            document: format!("{subject}\n\n{body}"),
        })
        .await
        .unwrap();

    storage
        .get_message(message_id)
        .await
        .unwrap()
        .unwrap()
        .thread_id
}

pub async fn seed() -> Seed {
    let dir = TempDir::new().unwrap();
    let storage = StorageHandle::open(dir.path(), 2).unwrap();
    let indexer = Arc::new(InMemoryIndexer::new());

    insert_account(&storage, OWNED_ID, OWNED_USERNAME).await;
    insert_account(&storage, FOREIGN_ID, FOREIGN_USERNAME).await;

    let owned_thread = insert_message(
        dir.path(),
        &storage,
        &indexer,
        OWNED_MSG,
        OWNED_ID,
        "quarterly report",
        "The quarterly report is ready for review.",
        1_777_000_000,
    )
    .await;
    let foreign_thread = insert_message(
        dir.path(),
        &storage,
        &indexer,
        FOREIGN_MSG,
        FOREIGN_ID,
        "secret report",
        "The secret report is classified material.",
        1_777_100_000,
    )
    .await;

    Seed {
        dir,
        storage,
        indexer,
        owned_thread,
        foreign_thread,
    }
}
