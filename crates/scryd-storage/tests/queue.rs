use scryd_storage::StorageHandle;
use tempfile::TempDir;

fn handle() -> (TempDir, StorageHandle) {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 1).expect("open");
    (dir, h)
}

#[tokio::test]
async fn pop_batch_orders_by_queued_at() {
    // Insert with explicit queued_at values rather than relying on sqlite's
    // second-resolution `strftime('%s','now')` and tokio::sleep timing.
    let (_d, h) = handle();
    h.with_writer(|conn| {
        conn.execute(
            "INSERT INTO index_queue (message_id, attempts, last_error, queued_at, failed_permanent) \
             VALUES ('primary:b@x', 0, NULL, 200, 0)",
            [],
        )?;
        conn.execute(
            "INSERT INTO index_queue (message_id, attempts, last_error, queued_at, failed_permanent) \
             VALUES ('primary:a@x', 0, NULL, 100, 0)",
            [],
        )?;
        conn.execute(
            "INSERT INTO index_queue (message_id, attempts, last_error, queued_at, failed_permanent) \
             VALUES ('primary:c@x', 0, NULL, 300, 0)",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let batch = h.pop_batch(10).await.unwrap();
    assert_eq!(batch.len(), 3);
    assert_eq!(batch[0].message_id, "primary:a@x");
    assert_eq!(batch[1].message_id, "primary:b@x");
    assert_eq!(batch[2].message_id, "primary:c@x");
}

#[tokio::test]
async fn enqueue_is_idempotent() {
    let (_d, h) = handle();
    h.enqueue("primary:dup@x").await.unwrap();
    h.enqueue("primary:dup@x").await.unwrap();
    let batch = h.pop_batch(10).await.unwrap();
    assert_eq!(batch.len(), 1);
}

#[tokio::test]
async fn delete_queue_row_removes_after_success() {
    let (_d, h) = handle();
    h.enqueue("primary:done@x").await.unwrap();
    h.delete_queue_row("primary:done@x").await.unwrap();
    assert!(h.pop_batch(10).await.unwrap().is_empty());
}

#[tokio::test]
async fn mark_failed_increments_until_permanent() {
    let (_d, h) = handle();
    h.enqueue("primary:flaky@x").await.unwrap();

    // Two attempts under the threshold; row stays poppable.
    let p1 = h
        .mark_failed("primary:flaky@x", "boom", 3)
        .await
        .unwrap();
    assert!(!p1, "first failure should not be permanent at max_attempts=3");
    let p2 = h
        .mark_failed("primary:flaky@x", "boom", 3)
        .await
        .unwrap();
    assert!(!p2, "second failure should not be permanent");

    let visible = h.pop_batch(10).await.unwrap();
    assert_eq!(visible.len(), 1, "row still drainable");
    assert_eq!(visible[0].attempts, 2);

    // Third failure crosses the ceiling.
    let p3 = h
        .mark_failed("primary:flaky@x", "boom", 3)
        .await
        .unwrap();
    assert!(p3, "third failure flips failed_permanent");

    let visible = h.pop_batch(10).await.unwrap();
    assert!(
        visible.is_empty(),
        "permanently-failed rows must not be returned by pop_batch"
    );

    // The row is still in the table for forensic inspection.
    let all = h.list_queue().await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].failed_permanent);
    assert_eq!(all[0].attempts, 3);
    assert_eq!(all[0].last_error.as_deref(), Some("boom"));
}

#[tokio::test]
async fn mark_failed_on_missing_row_is_no_op() {
    let (_d, h) = handle();
    let permanent = h.mark_failed("ghost", "x", 3).await.unwrap();
    assert!(!permanent);
}

#[tokio::test]
async fn pop_batch_respects_limit() {
    let (_d, h) = handle();
    for i in 0..5 {
        h.enqueue(&format!("primary:m{i}@x")).await.unwrap();
    }
    let batch = h.pop_batch(2).await.unwrap();
    assert_eq!(batch.len(), 2);
}
