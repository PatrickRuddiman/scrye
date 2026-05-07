use scryd_storage::{Address, MessageInsert, StorageHandle};
use tempfile::TempDir;

async fn fresh() -> (TempDir, StorageHandle) {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 1).unwrap();
    h.with_writer(|conn| {
        conn.execute(
            "INSERT INTO accounts (account_id, host, port, username, folders_json, active, mirrored_at) \
             VALUES ('primary', 'imap.example.com', 993, 'u', '[\"INBOX\"]', 1, 0)",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    (dir, h)
}

fn sample(id: &str, header_id: &str, uid: u32) -> MessageInsert {
    MessageInsert {
        message_id: id.to_string(),
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: uid,
        uidvalidity: 1,
        header_message_id: Some(header_id.to_string()),
        in_reply_to: None,
        references: vec![],
        sender_addr: "alice@example.com".to_string(),
        sender_name: None,
        recipients_to: vec![Address {
            addr: "bob@example.com".to_string(),
            name: None,
        }],
        recipients_cc: vec![],
        subject: Some("subj".to_string()),
        date_unix: 1_700_000_000,
        raw_path: "/tmp/x.eml".to_string(),
        body_md: "body".to_string(),
        size_bytes: 4,
    }
}

#[tokio::test]
async fn tombstone_marks_row_and_excludes_from_thread() {
    let (_d, h) = fresh().await;
    h.insert_message(sample("primary:a@x", "a@x", 1))
        .await
        .unwrap();
    h.insert_message(sample("primary:b@x", "b@x", 2))
        .await
        .unwrap();

    h.tombstone("primary:b@x").await.unwrap();
    let row = h.get_message("primary:b@x").await.unwrap().unwrap();
    assert!(row.tombstoned_at.is_some(), "tombstoned_at should be set");

    // get_thread filters tombstoned rows.
    let thread_id = h
        .get_message("primary:a@x")
        .await
        .unwrap()
        .unwrap()
        .thread_id;
    let in_thread = h.get_thread(&thread_id).await.unwrap();
    assert!(in_thread.iter().all(|r| r.message_id != "primary:b@x"));
}

#[tokio::test]
async fn tombstone_is_idempotent() {
    let (_d, h) = fresh().await;
    h.insert_message(sample("primary:a@x", "a@x", 1))
        .await
        .unwrap();
    h.tombstone("primary:a@x").await.unwrap();
    let first = h
        .get_message("primary:a@x")
        .await
        .unwrap()
        .unwrap()
        .tombstoned_at;

    // Second call must not overwrite the timestamp.
    h.tombstone("primary:a@x").await.unwrap();
    let second = h
        .get_message("primary:a@x")
        .await
        .unwrap()
        .unwrap()
        .tombstoned_at;
    assert_eq!(first, second);
}

#[tokio::test]
async fn reenqueue_all_messages_skips_tombstoned() {
    let (_d, h) = fresh().await;
    for i in 0..5u32 {
        h.insert_message(sample(
            &format!("primary:m{i}@x"),
            &format!("m{i}@x"),
            i + 1,
        ))
        .await
        .unwrap();
    }
    h.tombstone("primary:m2@x").await.unwrap();

    let n = h.reenqueue_all_messages().await.unwrap();
    assert_eq!(n, 4, "reenqueue should skip the 1 tombstoned row");

    let queue = h.list_queue().await.unwrap();
    assert_eq!(queue.len(), 4);
    assert!(queue.iter().all(|r| !r.failed_permanent));
    assert!(queue.iter().all(|r| r.attempts == 0));
    assert!(queue.iter().all(|r| r.message_id != "primary:m2@x"));
}

#[tokio::test]
async fn reenqueue_resets_attempts_and_failed_state() {
    let (_d, h) = fresh().await;
    h.insert_message(sample("primary:a@x", "a@x", 1))
        .await
        .unwrap();

    h.enqueue("primary:a@x").await.unwrap();
    h.mark_failed("primary:a@x", "boom", 1).await.unwrap();
    let before = h.list_queue().await.unwrap();
    assert!(before[0].failed_permanent, "row was permanently failed");

    h.reenqueue_all_messages().await.unwrap();
    let after = h.list_queue().await.unwrap();
    assert_eq!(after.len(), 1);
    assert!(!after[0].failed_permanent, "reenqueue clears permanent flag");
    assert_eq!(after[0].attempts, 0, "attempts reset to 0");
    assert!(after[0].last_error.is_none());
}
