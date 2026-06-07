use std::sync::Arc;

use scryd_imap::sink::{FetchedMessage, MessageSink, SyncStateUpdate};
use scryd_runtime::StorageMessageSink;
use scryd_storage::StorageHandle;
use tempfile::TempDir;
use tokio::sync::Notify;

const FIXTURE: &[u8] = b"From: Alice <alice@example.com>\r\n\
                         To: Bob <bob@example.com>\r\n\
                         Subject: hi\r\n\
                         Date: Mon, 12 Jan 2026 09:00:00 +0000\r\n\
                         Message-ID: <sink-fixture@example.com>\r\n\
                         Content-Type: text/plain; charset=utf-8\r\n\r\n\
                         Hello Bob, this is a plaintext body that exceeds the 100\
                         byte floor so the converter takes it as-is for indexing.\r\n";

async fn setup() -> (TempDir, StorageHandle, StorageMessageSink) {
    let dir = TempDir::new().unwrap();
    let storage = StorageHandle::open(dir.path(), 1).unwrap();
    storage
        .with_writer(|conn| {
            conn.execute(
                "INSERT INTO accounts (account_id, host, port, username, folders_json, active, mirrored_at) \
                 VALUES ('primary', 'imap.example.com', 993, 'u', '[\"INBOX\"]', 1, 0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let notify = Arc::new(Notify::new());
    let sink = StorageMessageSink::new(storage.clone(), notify, dir.path().to_path_buf());
    (dir, storage, sink)
}

#[tokio::test]
async fn submit_writes_message_row_raw_file_and_queue_entry() {
    let (_dir, storage, sink) = setup().await;
    let fetched = FetchedMessage {
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: 7,
        uidvalidity: 1,
        internal_date: Some(1_700_000_000),
        flags: vec![],
        raw_bytes: FIXTURE.to_vec(),
    };

    sink.submit(fetched).await.unwrap();

    let count: i64 = storage
        .with_reader(|c| {
            c.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);

    let raw_path: String = storage
        .with_reader(|c| {
            c.query_row("SELECT raw_path FROM messages", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    assert!(std::path::Path::new(&raw_path).exists(), "raw .eml at {raw_path}");

    let queue_count: i64 = storage
        .with_reader(|c| {
            c.query_row("SELECT COUNT(*) FROM index_queue", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(queue_count, 1);
}

#[tokio::test]
async fn submit_signals_drainer_via_notify() {
    let dir = TempDir::new().unwrap();
    let storage = StorageHandle::open(dir.path(), 1).unwrap();
    storage
        .with_writer(|conn| {
            conn.execute(
                "INSERT INTO accounts (account_id, host, port, username, folders_json, active, mirrored_at) \
                 VALUES ('primary', 'imap.example.com', 993, 'u', '[\"INBOX\"]', 1, 0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let notify = Arc::new(Notify::new());
    let sink = StorageMessageSink::new(storage.clone(), notify.clone(), dir.path().to_path_buf());

    // Pre-arm a notified() future so we can detect the wakeup.
    let notified = notify.clone();
    let watcher = tokio::spawn(async move { notified.notified().await });

    sink.submit(FetchedMessage {
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: 1,
        uidvalidity: 1,
        internal_date: None,
        flags: vec![],
        raw_bytes: FIXTURE.to_vec(),
    })
    .await
    .unwrap();

    tokio::time::timeout(std::time::Duration::from_millis(500), watcher)
        .await
        .expect("drainer notify should fire after submit")
        .unwrap();
}

#[tokio::test]
async fn tombstone_marks_existing_message() {
    let (_dir, storage, sink) = setup().await;
    let fetched = FetchedMessage {
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: 7,
        uidvalidity: 1,
        internal_date: None,
        flags: vec![],
        raw_bytes: FIXTURE.to_vec(),
    };
    sink.submit(fetched).await.unwrap();

    let id: String = storage
        .with_reader(|c| {
            c.query_row("SELECT message_id FROM messages", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    sink.tombstone(&id).await.unwrap();

    let row = storage.get_message(&id).await.unwrap().unwrap();
    assert!(row.tombstoned_at.is_some());
}

#[tokio::test]
async fn update_sync_state_persists_through_storage() {
    let (_dir, storage, sink) = setup().await;
    sink.update_sync_state(
        "primary",
        "INBOX",
        SyncStateUpdate {
            last_seen_uid: Some(42),
            uidvalidity: Some(1),
            account_health: Some("active".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let row = storage.get_sync_state("primary", "INBOX").await.unwrap().unwrap();
    assert_eq!(row.last_seen_uid, 42);
    assert_eq!(row.uidvalidity, Some(1));
    assert_eq!(row.account_health, scryd_storage::AccountHealth::Active);
}

#[tokio::test]
async fn parsed_degraded_still_writes_a_row() {
    // mail-parser is forgiving — most malformed input produces
    // ParsedDegraded with the bits it could recover. The runtime sink
    // accepts these and emits a parse-failure log per fault.
    let (_dir, storage, sink) = setup().await;
    let fetched = FetchedMessage {
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: 99,
        uidvalidity: 1,
        internal_date: Some(1_700_000_000),
        flags: vec![],
        raw_bytes: vec![0u8; 4], // mail-parser returns ParsedDegraded.
    };
    sink.submit(fetched).await.unwrap();

    // The placeholder still lands so the message is queryable by
    // (account, folder, uid).
    let count: i64 = storage
        .with_reader(|c| {
            c.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn resubmit_unchanged_message_does_not_reenqueue() {
    // Regression test for issue #12: normal IMAP sync re-fetches already-stored
    // messages and must NOT push them back onto the index queue.  The queue
    // would rebound after a partial drain and prevent reindex convergence.
    let (_dir, storage, sink) = setup().await;

    let fetched = FetchedMessage {
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: 42,
        uidvalidity: 1,
        internal_date: Some(1_700_000_000),
        flags: vec![],
        raw_bytes: FIXTURE.to_vec(),
    };

    // First submit — message is new, should be enqueued.
    sink.submit(fetched.clone()).await.unwrap();
    let after_first: i64 = storage
        .with_reader(|c| {
            c.query_row("SELECT COUNT(*) FROM index_queue", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(after_first, 1, "first submit should enqueue the message");

    // Simulate the drainer consuming the entry.
    let id: String = storage
        .with_reader(|c| {
            c.query_row("SELECT message_id FROM index_queue", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    storage.delete_queue_row(&id).await.unwrap();

    // Second submit with identical raw bytes (same body_md) — must not re-enqueue.
    sink.submit(fetched).await.unwrap();
    let after_second: i64 = storage
        .with_reader(|c| {
            c.query_row("SELECT COUNT(*) FROM index_queue", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(after_second, 0, "re-submit of unchanged message must not re-enqueue");
}
