//! Application state passed to every axum handler. Tasks 18–19 attach
//! request handlers that close over this struct.

use std::sync::Arc;
use std::time::Instant;

use scryd_config::Config;
use scryd_imap::Scheduler;
use scryd_search::{Indexer, Searcher};
use scryd_storage::StorageHandle;
use tokio::sync::{Mutex, RwLock};

/// Active reindex marker. The daemon-wide [`AppState::reindex_lock`] holds
/// at most one of these at a time.
#[derive(Debug)]
pub struct ReindexHandle {
    pub started_at: Instant,
    pub messages_count: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: StorageHandle,
    pub searcher: Arc<Searcher>,
    pub indexer: Arc<dyn Indexer>,
    pub reindex_lock: Arc<Mutex<Option<ReindexHandle>>>,
    /// Hot-reloadable config. The `/internal/reconcile` handler swaps a
    /// freshly-parsed `Config` in here so add-account flows are picked up
    /// without restarting the daemon.
    pub config: Arc<RwLock<Config>>,
    /// Daemon start time — surfaced through `GET /status` as
    /// `uptime_secs`.
    pub started_at: Instant,
    /// IMAP scheduler. `Some` in production (set by
    /// scryd-runtime::serve); `None` in api-only tests where the
    /// scheduler isn't constructed.
    pub scheduler: Option<Arc<Scheduler>>,
}

impl AppState {
    pub fn new(storage: StorageHandle, indexer: Arc<dyn Indexer>, config: Config) -> Self {
        Self::with_scheduler(storage, indexer, config, None)
    }

    pub fn with_scheduler(
        storage: StorageHandle,
        indexer: Arc<dyn Indexer>,
        config: Config,
        scheduler: Option<Arc<Scheduler>>,
    ) -> Self {
        let searcher = Arc::new(Searcher::new(indexer.clone()));
        Self {
            storage,
            searcher,
            indexer,
            reindex_lock: Arc::new(Mutex::new(None)),
            config: Arc::new(RwLock::new(config)),
            started_at: Instant::now(),
            scheduler,
        }
    }
}
