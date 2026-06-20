//! Scoped application state shared by every MCP tool.
//!
//! [`McpState`] holds exactly what the read/search surface needs (storage,
//! a [`Searcher`] over the daemon's indexer, the boot crash-loop health, and
//! the resolved [`AccountScope`](crate::scope::AccountScope) that bounds every
//! response to `USER_EMAIL`). The daemon owns the IMAP scheduler, drainer, and
//! reconcile-at-startup directly — there is no write/control surface on the
//! MCP boundary, so none of those handles leak into the tool state.

use std::sync::Arc;
use std::time::Instant;

use scryd_search::{Indexer, Searcher};
use scryd_storage::StorageHandle;

use crate::scope::AccountScope;

/// Crash-loop health computed once at daemon start (scryd-runtime::serve) and
/// surfaced read-only through the `status` tool. Defaults to a healthy
/// snapshot so tests and the first clean boot report `ok`. `Copy` so tools can
/// cheaply read it out of the cloned [`McpState`].
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

/// Cheaply-clonable, scope-bearing state every tool closes over.
#[derive(Clone)]
pub struct McpState {
    pub storage: StorageHandle,
    pub searcher: Arc<Searcher>,
    /// Daemon start time — surfaced through the `status` tool as `uptime_secs`.
    pub started_at: Instant,
    /// Crash-loop health captured at boot. `scryd-runtime::serve` overwrites
    /// this before serving; defaulted (healthy) everywhere else.
    pub daemon_health: DaemonHealthSnapshot,
    /// The `USER_EMAIL`-derived ownership boundary applied to every tool call.
    pub scope: AccountScope,
}

impl McpState {
    /// Build the scoped tool state. `indexer` is consumed to construct the
    /// [`Searcher`]; the daemon keeps its own clone for the drainer.
    pub fn new(storage: StorageHandle, indexer: Arc<dyn Indexer>, scope: AccountScope) -> Self {
        Self {
            storage,
            searcher: Arc::new(Searcher::new(indexer)),
            started_at: Instant::now(),
            daemon_health: DaemonHealthSnapshot::default(),
            scope,
        }
    }
}
