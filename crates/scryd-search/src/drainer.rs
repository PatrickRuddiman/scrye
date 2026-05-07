//! Long-lived async task that drains `meta.sqlite.index_queue`, hands each
//! row to the [`Indexer`], and applies the spec's attempts / failed_permanent
//! semantics. Logs `single-message-{full-text|semantic}-indexer-failure`
//! when a row crosses the permanent-failure ceiling.

use std::sync::Arc;
use std::time::Duration;

use scryd_log::{category, log_failure};
use scryd_storage::{IndexQueueRow, StorageError, StorageHandle};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::document::build as build_document;
use crate::indexer::Indexer;
use crate::{IndexError, IndexSubmit, MessageId};

/// Maximum failed attempts before a queue row is flagged
/// `failed_permanent` and removed from the drain rotation.
pub const MAX_ATTEMPTS: u32 = 5;

/// Number of queue rows the drainer pulls per loop iteration.
pub const INDEXER_BATCH: usize = 32;

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
        loop {
            if self.shutdown.is_cancelled() {
                return;
            }
            match self.tick().await {
                Ok(0) => {
                    // Queue empty; wait for a notify, a poll timer, or shutdown.
                    tokio::select! {
                        _ = self.notify.notified() => {}
                        _ = tokio::time::sleep(IDLE_POLL) => {}
                        _ = self.shutdown.cancelled() => return,
                    }
                }
                Ok(_n) => {
                    // We just made progress; loop again immediately.
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
