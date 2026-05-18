//! Drainer unit tests.
//!
//! Uses an in-process stub `Indexer` to control success/failure on specific
//! message ids; the witchcraft-backed integration test that the slice
//! mentions is gated to the follow-up that wires up the production binding.

use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use scryd_search::drainer::{Drainer, INDEXER_BATCH, INDEX_REBUILD_THRESHOLD, MAX_ATTEMPTS};
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

/// Indexer stub that wraps `InMemoryIndexer` and counts the v0.3.6
/// split flush operations. Mimics the witchcraft semantics:
/// `submit` adds to a pending-embed pool; `flush_embeddings`
/// drains up to a fixed slice each call and reports the count;
/// `flush_index` runs the (here-fake) cluster pass.
///
/// The drainer's correctness contract is:
/// - call `flush_embeddings` after every non-empty tick (so new
///   docs become searchable),
/// - call `flush_index` on idle if anything was embedded, or once
///   accumulated embeds cross `INDEX_REBUILD_THRESHOLD`,
/// - never call `flush_index` on a tick that did no embed work.
struct CountingFlushStub {
    inner: InMemoryIndexer,
    flush_embeddings_calls: AtomicUsize,
    flush_index_calls: AtomicUsize,
    embedded_total: AtomicUsize,
    pending_embed: AtomicUsize,
    // Max docs `flush_embeddings` claims to embed per call. Mirrors
    // FLUSH_EMBED_BATCH in the witchcraft binding (4).
    embed_slice: usize,
}

impl CountingFlushStub {
    fn new() -> Self {
        Self::with_embed_slice(4)
    }
    fn with_embed_slice(embed_slice: usize) -> Self {
        Self {
            inner: InMemoryIndexer::new(),
            flush_embeddings_calls: AtomicUsize::new(0),
            flush_index_calls: AtomicUsize::new(0),
            embedded_total: AtomicUsize::new(0),
            pending_embed: AtomicUsize::new(0),
            embed_slice,
        }
    }
    fn flush_embeddings_count(&self) -> usize {
        self.flush_embeddings_calls.load(Ordering::SeqCst)
    }
    fn flush_index_count(&self) -> usize {
        self.flush_index_calls.load(Ordering::SeqCst)
    }
    fn embedded_total(&self) -> usize {
        self.embedded_total.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Indexer for CountingFlushStub {
    async fn submit(&self, submit: IndexSubmit) -> Result<(), IndexError> {
        self.inner.submit(submit).await?;
        self.pending_embed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn remove(&self, id: &MessageId) -> Result<(), IndexError> {
        self.inner.remove(id).await
    }
    async fn truncate(&self) -> Result<(), IndexError> {
        self.inner.truncate().await
    }
    async fn search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError> {
        self.inner.search(query).await
    }
    async fn flush_embeddings(&self) -> Result<usize, IndexError> {
        self.flush_embeddings_calls.fetch_add(1, Ordering::SeqCst);
        let current = self.pending_embed.load(Ordering::SeqCst);
        let to_embed = current.min(self.embed_slice);
        if to_embed > 0 {
            self.pending_embed.fetch_sub(to_embed, Ordering::SeqCst);
            self.embedded_total.fetch_add(to_embed, Ordering::SeqCst);
        }
        Ok(to_embed)
    }
    async fn flush_index(&self) -> Result<(), IndexError> {
        self.flush_index_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
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
async fn drainer_calls_flush_embeddings_after_nonempty_tick() {
    // v0.3.6 split flush_pending into flush_embeddings (per tick)
    // and flush_index (on idle / threshold). Regression: if the
    // per-tick embed call is removed, new docs would stop becoming
    // searchable until the next idle window.
    let (_d, storage) = fresh_storage().await;
    let indexer = Arc::new(CountingFlushStub::new());

    storage
        .insert_message(sample("primary:flush@x", 1))
        .await
        .unwrap();
    storage.enqueue("primary:flush@x").await.unwrap();

    let drainer = Drainer::new(storage.clone(), indexer.clone());
    let token = drainer.shutdown_token();
    let notify = drainer.enqueue_notify();
    let handle = tokio::spawn(drainer.run());

    notify.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    token.cancel();
    handle.await.unwrap();

    assert!(
        indexer.flush_embeddings_count() >= 1,
        "drainer must call flush_embeddings at least once after a non-empty tick (got {})",
        indexer.flush_embeddings_count()
    );
    assert_eq!(
        indexer.embedded_total(),
        1,
        "the single submitted doc must have been embedded"
    );
}

#[tokio::test]
async fn drainer_calls_flush_index_on_idle_after_embeds() {
    // v0.3.6: with the queue drained and at least one doc embedded
    // since the last cluster pass, the idle transition fires
    // flush_index exactly once before sleeping on notify.
    let (_d, storage) = fresh_storage().await;
    let indexer = Arc::new(CountingFlushStub::new());

    storage
        .insert_message(sample("primary:idle@x", 1))
        .await
        .unwrap();
    storage.enqueue("primary:idle@x").await.unwrap();

    let drainer = Drainer::new(storage.clone(), indexer.clone());
    let token = drainer.shutdown_token();
    let notify = drainer.enqueue_notify();
    let handle = tokio::spawn(drainer.run());

    notify.notify_one();
    // Long enough for: drain tick, embed pass, idle tick fires
    // flush_index, then we cancel.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    token.cancel();
    handle.await.unwrap();

    assert_eq!(
        indexer.flush_index_count(),
        1,
        "idle path should fire flush_index exactly once after embedding (got {})",
        indexer.flush_index_count()
    );
}

#[tokio::test]
async fn drainer_does_not_call_flush_index_when_nothing_was_embedded() {
    // Idle path is gated on `embeds_since_last_index > 0`, so an
    // idle drainer that has done no embed work since the last
    // cluster pass should NOT trigger a fresh flush_index. The
    // witchcraft impl is internally threshold-gated anyway, but
    // skipping the call entirely keeps the logs quieter and avoids
    // the cost of taking the state mutex.
    let (_d, storage) = fresh_storage().await;
    let indexer = Arc::new(CountingFlushStub::new());

    // Empty queue → tick returns Ok(0) immediately → idle branch
    // sees embeds_since_last_index = 0 → no flush_index.
    let drainer = Drainer::new(storage.clone(), indexer.clone());
    let token = drainer.shutdown_token();
    let handle = tokio::spawn(drainer.run());

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    token.cancel();
    handle.await.unwrap();

    assert_eq!(
        indexer.flush_index_count(),
        0,
        "flush_index must not run when there's been no embed work (got {})",
        indexer.flush_index_count()
    );
    assert_eq!(indexer.flush_embeddings_count(), 0);
}

#[test]
fn index_rebuild_threshold_amortizes_cascade() {
    // INDEX_REBUILD_THRESHOLD caps how stale clustering can get
    // during a never-idle backfill. Constraint: it must span
    // multiple INDEXER_BATCH drains so we don't trigger the
    // cascade on near-every batch (the v0.3.5 bug), but also not
    // be so high that backfills always rely on idle (a backfill
    // that legitimately never idles needs *some* periodic
    // cascade).
    assert!(
        INDEX_REBUILD_THRESHOLD >= INDEXER_BATCH * 4,
        "threshold {INDEX_REBUILD_THRESHOLD} must span ≥4 drainer batches \
         to amortize the cluster cascade"
    );
    assert!(
        INDEX_REBUILD_THRESHOLD <= 1024,
        "threshold {INDEX_REBUILD_THRESHOLD} too high — backfill would never \
         trigger the threshold path before going idle"
    );
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
            account_ids: Vec::new(),
    };
}
