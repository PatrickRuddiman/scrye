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

/// Crash-loop health computed once at daemon start (scryd-runtime::serve) and
/// surfaced read-only through `GET /status`. Defaults to a healthy snapshot so
/// api-only tests and the first clean boot report `ok`. `Copy` so handlers can
/// cheaply read it out of the cloned [`AppState`].
#[derive(Clone, Copy, Debug, Default)]
pub struct DaemonHealthSnapshot {
    /// Monotonic lifetime restart counter (`daemon_runs.run_id` of this run).
    pub restart_count: i64,
    /// Consecutive preceding runs that died without a clean shutdown.
    pub consecutive_crashes: u32,
    /// `started_at` of the most recent unclean run, if any.
    pub last_crash_unix: Option<i64>,
    /// True when startup detected a crash loop and applied backoff.
    pub in_crash_loop: bool,
    /// Unix time the startup backoff sleep was scheduled to end, if any.
    pub backoff_until_unix: Option<i64>,
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
    /// Crash-loop health captured at boot. `scryd-runtime::serve` overwrites
    /// this before serving; defaulted (healthy) everywhere else.
    pub daemon_health: DaemonHealthSnapshot,
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
            daemon_health: DaemonHealthSnapshot::default(),
        }
    }
}
