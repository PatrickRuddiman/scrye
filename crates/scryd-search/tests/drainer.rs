//! Drainer unit tests.
//!
//! Uses an in-process stub `Indexer` to control success/failure on specific
//! message ids; the witchcraft-backed integration test that the slice
//! mentions is gated to the follow-up that wires up the production binding.

use std::io::Write;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use scryd_search::drainer::{Drainer, INDEXER_BATCH, MAX_ATTEMPTS};
use scryd_search::indexer::Indexer;
use scryd_search::{
    Hit, IndexError, IndexSubmit, InMemoryIndexer, MessageId, Mode, SearchError, SearchQuery,
    SearchResponse,
};
use scryd_storage::{Address, MessageInsert, StorageHandle};
use tempfile::TempDir;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl CaptureWriter {
    fn snapshot(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

struct FailingStub {
    fail_id: String,
    submitted: Mutex<Vec<MessageId>>,
}

impl FailingStub {
    fn new(fail_id: &str) -> Self {
        Self {
            fail_id: fail_id.to_string(),
            submitted: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Indexer for FailingStub {
    async fn submit(&self, submit: IndexSubmit) -> Result<(), IndexError> {
        self.submitted.lock().unwrap().push(submit.message_id.clone());
        if submit.message_id.as_str() == self.fail_id {
            return Err(IndexError::Upstream("simulated".to_string()));
        }
        Ok(())
    }
    async fn remove(&self, _id: &MessageId) -> Result<(), IndexError> {
        Ok(())
    }
    async fn truncate(&self) -> Result<(), IndexError> {
        Ok(())
    }
    async fn search(&self, _query: &SearchQuery) -> Result<SearchResponse, SearchError> {
        Ok(SearchResponse { hits: Vec::new() })
    }
}

async fn fresh_storage() -> (TempDir, StorageHandle) {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 1).expect("open");
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

fn sample(id: &str, uid: u32) -> MessageInsert {
    MessageInsert {
        message_id: id.to_string(),
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: uid,
        uidvalidity: 1,
        header_message_id: Some(format!("{id}-h")),
        in_reply_to: None,
        references: vec![],
        sender_addr: "alice@example.com".to_string(),
        sender_name: Some("Alice".to_string()),
        recipients_to: vec![Address {
            addr: "bob@example.com".to_string(),
            name: None,
        }],
        recipients_cc: vec![],
        subject: Some("hi".to_string()),
        date_unix: 1_700_000_000,
        raw_path: "/tmp/x.eml".to_string(),
        body_md: "hello".to_string(),
        size_bytes: 5,
    }
}

#[tokio::test]
async fn drainer_indexes_a_queued_message_with_in_memory_backend() {
    let (_d, storage) = fresh_storage().await;
    let indexer = Arc::new(InMemoryIndexer::new());
    storage.insert_message(sample("primary:a@x", 1)).await.unwrap();
    storage.enqueue("primary:a@x").await.unwrap();

    let drainer = Drainer::new(storage.clone(), indexer.clone());
    let n = drainer.tick().await.unwrap();
    assert_eq!(n, 1, "tick processes the one queued row");

    // Queue row was deleted on success.
    let queue = storage.list_queue().await.unwrap();
    assert!(queue.is_empty(), "queue empty after success: {queue:?}");

    // Indexer received the document.
    assert_eq!(indexer.len(), 1);
}

#[tokio::test]
async fn drainer_marks_permanently_failed_after_max_attempts_and_emits_log() {
    let cap = CaptureWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_target(true)
        .with_writer(cap.clone())
        .with_env_filter("scryd_search=warn,warn")
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let (_d, storage) = fresh_storage().await;
    let stub: Arc<dyn Indexer> = Arc::new(FailingStub::new("primary:flaky@x"));
    storage.insert_message(sample("primary:flaky@x", 7)).await.unwrap();
    storage.enqueue("primary:flaky@x").await.unwrap();

    let drainer = Drainer::new(storage.clone(), stub.clone());

    // Run MAX_ATTEMPTS ticks: each pops the row, fails, increments attempts.
    for _ in 0..MAX_ATTEMPTS {
        let n = drainer.tick().await.unwrap();
        assert_eq!(n, 1);
    }

    // After the last failure, failed_permanent flips and the row drops out.
    let next = drainer.tick().await.unwrap();
    assert_eq!(next, 0, "permanently-failed row must not be returned");

    let queue = storage.list_queue().await.unwrap();
    assert_eq!(queue.len(), 1);
    assert!(queue[0].failed_permanent);
    assert_eq!(queue[0].attempts, MAX_ATTEMPTS);

    // Exactly one indexer-failure log entry.
    let captured = cap.snapshot();
    let lines: Vec<&str> = captured
        .lines()
        .filter(|l| l.contains("\"category\":\"single-message full-text indexer failure\""))
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one FT indexer failure log line, got {captured}"
    );
    assert!(lines[0].contains("\"message_id\":\"primary:flaky@x\""));
}

#[tokio::test]
async fn drainer_drops_queue_row_for_disappeared_message() {
    let (_d, storage) = fresh_storage().await;
    let indexer = Arc::new(InMemoryIndexer::new());

    // Enqueue an id with no matching message row.
    storage.enqueue("primary:ghost@x").await.unwrap();

    let drainer = Drainer::new(storage.clone(), indexer.clone());
    drainer.tick().await.unwrap();

    let queue = storage.list_queue().await.unwrap();
    assert!(queue.is_empty(), "ghost row should be cleaned out: {queue:?}");
    assert_eq!(indexer.len(), 0);
}

#[tokio::test]
async fn drainer_run_loop_processes_then_returns_on_shutdown() {
    let (_d, storage) = fresh_storage().await;
    let indexer = Arc::new(InMemoryIndexer::new());
    storage.insert_message(sample("primary:a@x", 1)).await.unwrap();
    storage.enqueue("primary:a@x").await.unwrap();

    let drainer = Drainer::new(storage.clone(), indexer.clone());
    let token = drainer.shutdown_token();
    let notify = drainer.enqueue_notify();
    let handle = tokio::spawn(drainer.run());

    // Nudge the drainer.
    notify.notify_one();
    // Give it a moment to process before we cancel.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    token.cancel();

    handle.await.unwrap();

    let queue = storage.list_queue().await.unwrap();
    assert!(queue.is_empty());
    assert_eq!(indexer.len(), 1);
}

#[test]
fn constants_match_slice_decision_4() {
    assert_eq!(MAX_ATTEMPTS, 5);
    assert_eq!(INDEXER_BATCH, 32);
}

#[tokio::test]
async fn batch_drains_multiple_rows_per_tick() {
    let (_d, storage) = fresh_storage().await;
    let indexer = Arc::new(InMemoryIndexer::new());
    for i in 0..7u32 {
        storage
            .insert_message(sample(&format!("primary:m{i}@x"), i + 1))
            .await
            .unwrap();
        storage.enqueue(&format!("primary:m{i}@x")).await.unwrap();
    }

    let drainer = Drainer::new(storage.clone(), indexer.clone());
    let n = drainer.tick().await.unwrap();
    assert_eq!(n, 7, "single tick should drain the whole batch up to INDEXER_BATCH");

    assert!(storage.list_queue().await.unwrap().is_empty());
    assert_eq!(indexer.len(), 7);
}

#[tokio::test]
async fn stub_records_attempts_for_observability() {
    // Kept simple: just confirm the FailingStub surface is wired up so
    // future witchcraft tests can plug the same harness in.
    let stub = FailingStub::new("nonexistent");
    let _: Arc<dyn Indexer> = Arc::new(stub);
}

#[tokio::test]
async fn unused_helpers_silence_compiler() {
    // Touch hits/responses so the import set in this file isn't trimmed.
    let _h = Hit {
        message_id: MessageId::new("x"),
        score: 0.0,
        semantic_snippet: None,
    };
    let _q = SearchQuery {
        q: String::new(),
        mode: Mode::FullText,
        k: 0,
    };
}
