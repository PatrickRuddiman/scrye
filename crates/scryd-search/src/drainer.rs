//! Long-lived async task that drains `meta.sqlite.index_queue`, hands each
//! row to the [`Indexer`], and applies the spec's attempts / failed_permanent
//! semantics. Logs `single-message-{full-text|semantic}-indexer-failure`
//! when a row crosses the permanent-failure ceiling.

use std::sync::Arc;
use std::time::{Duration, Instant};

use scryd_log::{category, log_failure};
use scryd_storage::{IndexQueueRow, StorageError, StorageHandle};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::document::build as build_document;
use crate::indexer::Indexer;
use crate::{IndexError, IndexSubmit, MessageId};

/// Maximum failed attempts before a queue row is flagged
/// `failed_permanent` and removed from the drain rotation.
pub const MAX_ATTEMPTS: u32 = 5;

/// Number of queue rows the drainer pulls per loop iteration.
pub const INDEXER_BATCH: usize = 32;

/// Accumulated docs embedded since the last `flush_index` call
/// before the drainer forces another cascade. Caps how stale
/// clustering can get during a multi-hour backfill that never goes
/// idle. With ~580 embeddings/doc on XTR multi-vector, 256 docs
/// ≈ 150k embeddings — comfortably above witchcraft's L2 cascade
/// threshold so each cascade we do trigger does meaningful work
/// rather than churning L0 over and over.
pub const INDEX_REBUILD_THRESHOLD: usize = 256;

/// Idle poll cadence when the queue is empty and no enqueue notification has
/// arrived. Bounded so the drainer wakes up periodically even if the
/// notify-from-storage path is lost (test runtime, dev), but small enough to
/// keep the spec's IDLE-to-queryable latency budget.
const IDLE_POLL: Duration = Duration::from_secs(1);

pub struct Drainer {
    storage: StorageHandle,
    indexer: Arc<dyn Indexer>,
    shutdown: CancellationToken,
    notify: Arc<Notify>,
}

impl Drainer {
    pub fn new(storage: StorageHandle, indexer: Arc<dyn Indexer>) -> Self {
        Self {
            storage,
            indexer,
            shutdown: CancellationToken::new(),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Hand to storage's insert path so newly-enqueued rows wake the drainer.
    pub fn enqueue_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    /// Hand to the runtime so SIGTERM cleanly stops the loop.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Cooperative shutdown — cancels the loop's next iteration.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    /// Drain forever. Returns when `shutdown_token().cancel()` is called.
    pub async fn run(self) {
        // Counts docs the indexer has embedded since the last
        // `flush_index` call. Crosses `INDEX_REBUILD_THRESHOLD` →
        // we force a cascade even though the queue isn't idle, so
        // a multi-hour backfill doesn't leave clustering arbitrarily
        // far behind.
        let mut embeds_since_last_index: usize = 0;
        loop {
            if self.shutdown.is_cancelled() {
                return;
            }
            let tick_started = Instant::now();
            match self.tick().await {
                Ok(0) => {
                    // Queue empty. Take this opportunity to run the
                    // clustering cascade if we have any pending embed
                    // work since the last one. `flush_index` is a
                    // cheap no-op below witchcraft's L0_CAPACITY so
                    // calling it on every idle transition is fine.
                    if embeds_since_last_index > 0 {
                        self.run_flush_index().await;
                        embeds_since_last_index = 0;
                    }
                    // Then wait for a notify, a poll timer, or shutdown.
                    tokio::select! {
                        _ = self.notify.notified() => {}
                        _ = tokio::time::sleep(IDLE_POLL) => {}
                        _ = self.shutdown.cancelled() => return,
                    }
                }
                Ok(n) => {
                    let tick_elapsed_ms = tick_started.elapsed().as_millis() as u64;
                    info!(
                        target: "scryd_search::drainer",
                        batch = n,
                        elapsed_ms = tick_elapsed_ms,
                        "drained batch from index_queue"
                    );
                    // Embed in a bounded loop so each individual call
                    // releases the indexer's state mutex (~20 s max
                    // hold) before the next one. Caps at 8 rounds —
                    // that's INDEXER_BATCH / FLUSH_EMBED_BATCH = 32/4
                    // worth of catchup; if we still have more dirty
                    // docs the next drainer iteration will keep going.
                    for _ in 0..(INDEXER_BATCH / 4).max(1) {
                        match self.indexer.flush_embeddings().await {
                            Ok(0) => break,
                            Ok(embedded) => {
                                embeds_since_last_index =
                                    embeds_since_last_index.saturating_add(embedded);
                                info!(
                                    target: "scryd_search::drainer",
                                    embedded = embedded,
                                    pending_for_index = embeds_since_last_index,
                                    "flush_embeddings completed"
                                );
                            }
                            Err(e) => {
                                warn!(
                                    target: "scryd_search::drainer",
                                    error = %e,
                                    "flush_embeddings failed; continuing"
                                );
                                break;
                            }
                        }
                    }
                    // Only fire the (potentially expensive) clustering
                    // cascade when enough work has piled up to amortize
                    // it. The idle path above handles the steady-state
                    // "eventually flush" case.
                    if embeds_since_last_index >= INDEX_REBUILD_THRESHOLD {
                        self.run_flush_index().await;
                        embeds_since_last_index = 0;
                    }
                    // Loop again immediately to drain more rows if present.
                }
                Err(e) => {
                    warn!(target: "scryd_search::drainer", error = %e, "queue read failed; backing off");
                    tokio::select! {
                        _ = tokio::time::sleep(IDLE_POLL) => {}
                        _ = self.shutdown.cancelled() => return,
                    }
                }
            }
        }
    }

    /// Run the indexer's cluster pass with timing + error logging.
    /// Extracted so the idle and threshold paths share the same
    /// reporting shape.
    async fn run_flush_index(&self) {
        let started = Instant::now();
        match self.indexer.flush_index().await {
            Ok(()) => {
                info!(
                    target: "scryd_search::drainer",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "flush_index completed"
                );
            }
            Err(e) => {
                warn!(
                    target: "scryd_search::drainer",
                    error = %e,
                    "flush_index failed; continuing"
                );
            }
        }
    }

    /// Process up to one batch. Returns the number of rows attempted.
    /// Public for unit testing — production callers use [`run`].
    pub async fn tick(&self) -> Result<usize, StorageError> {
        let rows = self.storage.pop_batch(INDEXER_BATCH).await?;
        let n = rows.len();
        for row in rows {
            self.process(row).await;
        }
        Ok(n)
    }

    async fn process(&self, row: IndexQueueRow) {
        let id = row.message_id.clone();

        let message = match self.storage.get_message(&id).await {
            Ok(Some(m)) => m,
            Ok(None) => {
                // Message disappeared since enqueue; drop the queue row.
                let _ = self.storage.delete_queue_row(&id).await;
                return;
            }
            Err(e) => {
                warn!(
                    target: "scryd_search::drainer",
                    message_id = %id,
                    error = %e,
                    "failed to load message for indexing"
                );
                return;
            }
        };

        let document = build_document(
            message.subject.as_deref(),
            &message.sender_addr,
            message.sender_name.as_deref(),
            &message.body_md,
        );

        let submit = IndexSubmit {
            message_id: MessageId::new(id.clone()),
            document,
        };

        match self.indexer.submit(submit).await {
            Ok(()) => {
                if let Err(e) = self.storage.delete_queue_row(&id).await {
                    warn!(
                        target: "scryd_search::drainer",
                        message_id = %id,
                        error = %e,
                        "failed to delete drained queue row"
                    );
                }
            }
            Err(err) => {
                let err_str = err.to_string();
                let permanent = self
                    .storage
                    .mark_failed(&id, &err_str, MAX_ATTEMPTS)
                    .await
                    .unwrap_or(false);
                if permanent {
                    let cat = category_for_error(&err);
                    log_failure!(
                        severity = warn,
                        category = cat,
                        message_id = %id,
                        error = %err_str
                    );
                }
            }
        }
    }
}

/// Pick the spec failure category for a permanently-failed indexer error.
///
/// v1 always returns the full-text category; the witchcraft binding will
/// extend [`IndexError`] with a SemanticOnly variant and route those here
/// to [`category::SINGLE_MESSAGE_SEMANTIC_INDEXER_FAILURE`].
fn category_for_error(err: &IndexError) -> &'static str {
    if is_semantic_only(err) {
        category::SINGLE_MESSAGE_SEMANTIC_INDEXER_FAILURE
    } else {
        category::SINGLE_MESSAGE_FT_INDEXER_FAILURE
    }
}

fn is_semantic_only(_err: &IndexError) -> bool {
    // No error variant flags the embedding pipeline specifically in v1.
    // When the witchcraft binding lands, match against that variant here.
    false
}
