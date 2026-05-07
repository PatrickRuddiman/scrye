use scryd_storage::{Address, MessageInsert, StorageHandle};
use tempfile::TempDir;

fn handle() -> (TempDir, StorageHandle) {
    let dir = TempDir::new().unwrap();
    // Seed an accounts row so FK validates.
    let h = StorageHandle::open(dir.path(), 2).expect("open");
    let _ = tokio::runtime::Handle::try_current(); // not in async ctx; ignore
    (dir, h)
}

async fn seed_account(h: &StorageHandle, id: &str) {
    h.with_writer(|conn| {
        conn.execute(
            "INSERT INTO accounts (account_id, host, port, username, folders_json, active, mirrored_at) \
             VALUES (?1, 'imap.example.com', 993, 'u', '[\"INBOX\"]', 1, 0)",
            rusqlite::params![id],
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

fn sample_insert(message_id: &str, header_id: Option<&str>) -> MessageInsert {
    MessageInsert {
        message_id: message_id.to_string(),
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: 42,
        uidvalidity: 1,
        header_message_id: header_id.map(|s| s.to_string()),
        in_reply_to: None,
        references: vec![],
        sender_addr: "alice@example.com".to_string(),
        sender_name: Some("Alice".to_string()),
        recipients_to: vec![Address {
            addr: "bob@example.com".to_string(),
            name: None,
        }],
        recipients_cc: vec![],
        subject: Some("Hello".to_string()),
        date_unix: 1_700_000_000,
        raw_path: "/tmp/x.eml".to_string(),
        body_md: "hi there".to_string(),
        size_bytes: 9,
    }
}

#[tokio::test]
async fn insert_and_fetch_roundtrip() {
    let (_d, h) = handle();
    seed_account(&h, "primary").await;

    let mut m = sample_insert("primary:abc@x", Some("abc@x"));
    m.server_uid = 100;
    h.insert_message(m).await.unwrap();

    let row = h.get_message("primary:abc@x").await.unwrap().unwrap();
    assert_eq!(row.account_id, "primary");
    assert_eq!(row.subject.as_deref(), Some("Hello"));
    assert_eq!(row.recipients_to.len(), 1);
    assert_eq!(row.recipients_to[0].addr, "bob@example.com");
}

#[tokio::test]
async fn reinsert_does_not_duplicate() {
    let (_d, h) = handle();
    seed_account(&h, "primary").await;

    let mut m1 = sample_insert("primary:dup@x", Some("dup@x"));
    m1.server_uid = 7;
    h.insert_message(m1).await.unwrap();

    let mut m2 = sample_insert("primary:dup@x", Some("dup@x"));
    m2.server_uid = 7;
    m2.body_md = "updated body".to_string();
    h.insert_message(m2).await.unwrap();

    // ON CONFLICT updates body_md.
    let row = h.get_message("primary:dup@x").await.unwrap().unwrap();
    assert_eq!(row.body_md, "updated body");

    let count: i64 = h
        .with_reader(|conn| {
            conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
                .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn thread_resolution_groups_replies() {
    let (_d, h) = handle();
    seed_account(&h, "primary").await;

    // Parent message.
    let mut parent = sample_insert("primary:parent@x", Some("parent@x"));
    parent.server_uid = 1;
    h.insert_message(parent).await.unwrap();

    // Child references the parent's header_message_id.
    let mut child = sample_insert("primary:child@x", Some("child@x"));
    child.server_uid = 2;
    child.in_reply_to = Some("parent@x".to_string());
    h.insert_message(child).await.unwrap();

    let p = h.get_message("primary:parent@x").await.unwrap().unwrap();
    let c = h.get_message("primary:child@x").await.unwrap().unwrap();
    assert_eq!(p.thread_id, c.thread_id, "child should join parent's thread");
}

#[tokio::test]
async fn get_thread_orders_oldest_first_and_excludes_tombstones() {
    let (_d, h) = handle();
    seed_account(&h, "primary").await;

    let mut a = sample_insert("primary:a@x", Some("a@x"));
    a.server_uid = 1;
    a.date_unix = 100;
    h.insert_message(a).await.unwrap();

    let mut b = sample_insert("primary:b@x", Some("b@x"));
    b.server_uid = 2;
    b.date_unix = 200;
    b.in_reply_to = Some("a@x".to_string());
    h.insert_message(b).await.unwrap();

    let mut c = sample_insert("primary:c@x", Some("c@x"));
    c.server_uid = 3;
    c.date_unix = 300;
    c.in_reply_to = Some("a@x".to_string());
    h.insert_message(c).await.unwrap();

    let thread_id = h.get_message("primary:a@x").await.unwrap().unwrap().thread_id;
    let rows = h.get_thread(&thread_id).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].message_id, "primary:a@x");
    assert_eq!(rows[2].message_id, "primary:c@x");

    h.tombstone("primary:b@x").await.unwrap();
    let rows = h.get_thread(&thread_id).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.message_id != "primary:b@x"));
}

#[tokio::test]
async fn get_message_returns_none_for_unknown_id() {
    let (_d, h) = handle();
    assert!(h.get_message("nope:nope@x").await.unwrap().is_none());
}
