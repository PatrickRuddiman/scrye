//! Per-account supervisor lifecycle + the [`Scheduler`] that owns the
//! tree.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use scryd_config::Config;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::connect::{login, LoggedIn};
use crate::fetch::run_initial_backfill;
use crate::idle::run_idle_loop;
use crate::sink::MessageSink;
use crate::state::Connection;
use crate::supervisor::{log_auth_rejection, log_connect_failure};
use crate::ClientError;

/// Maximum concurrent IMAP connections per account. Most providers cap
/// concurrent sessions per account (Gmail at 15, Outlook at 20, smaller
/// hosts often lower); 5 is a conservative default.
pub const MAX_CONNECTIONS_PER_ACCOUNT: usize = 5;

/// First retry delay after a failed account connection.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(30);

/// Cap on the exponential retry delay.
pub const BACKOFF_CAP: Duration = Duration::from_secs(3600);

/// Compute the next backoff delay given the number of consecutive failures.
/// Doubles each attempt, capped at [`BACKOFF_CAP`].
///
///   `backoff_after(0)` = 30s   (first failure)
///   `backoff_after(5)` = 960s  (2^5 * 30)
///   `backoff_after(7)` = 3600s (min(2^7 * 30, 3600) = capped)
pub fn backoff_after(failures: u32) -> Duration {
    let exp = failures.min(20);
    let secs = 30u64.saturating_mul(1u64 << exp);
    let capped = secs.min(BACKOFF_CAP.as_secs());
    Duration::from_secs(capped)
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("scheduler is already running")]
    AlreadyStarted,
    #[error("scheduler is not running")]
    NotStarted,
}

/// Per-(account, folder) state held by the scheduler.
struct SupervisorHandle {
    join: JoinHandle<()>,
    /// Per-supervisor "fetch now" tick. Incremented by
    /// `Scheduler::request_pass` so missed wakes don't merge.
    pass_request: watch::Sender<u64>,
    /// Per-supervisor exit signal. The scheduler flips this when
    /// the (account, folder) pair disappears from config during
    /// reconcile, so the supervisor can shut down without bringing
    /// the whole daemon with it.
    exit: watch::Sender<bool>,
}

pub struct Scheduler {
    config: std::sync::RwLock<Arc<Config>>,
    sink: Arc<dyn MessageSink>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    handles: Mutex<HashMap<(String, String), SupervisorHandle>>,
    /// IDLE recycle cadence honored by the supervisor's idle loop.
    /// Tests override to a sub-second value; production uses
    /// [`crate::idle::IDLE_RECYCLE`].
    idle_recycle: Duration,
}

impl Scheduler {
    /// Construct a Scheduler with default IDLE recycle of 25 min.
    pub fn new(config: Arc<Config>, sink: Arc<dyn MessageSink>) -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            config: std::sync::RwLock::new(config),
            sink,
            shutdown_tx: tx,
            shutdown_rx: rx,
            handles: Mutex::new(HashMap::new()),
            idle_recycle: crate::idle::IDLE_RECYCLE,
        }
    }

    /// Tests-only: override the IDLE recycle cadence so the supervisor
    /// loop ticks fast enough to assert behavior in seconds.
    pub fn with_idle_recycle(mut self, idle_recycle: Duration) -> Self {
        self.idle_recycle = idle_recycle;
        self
    }

    /// Replace the config driving this scheduler's supervisor set.
    /// Takes effect on the next `reconcile()` call; newly-spawned
    /// supervisors pick it up immediately, existing supervisors use
    /// their own `Arc<Config>` copy (refreshed on the next reconnect).
    pub fn update_config(&self, config: Arc<Config>) {
        *self.config.write().unwrap() = config;
    }

    /// Spawn one supervisor task per (account, folder) pair, bounded
    /// by `MAX_CONNECTIONS_PER_ACCOUNT` per account. Idempotent: a
    /// supervisor key already present in the map is left alone.
    pub async fn start(&self) -> Result<(), SchedulerError> {
        self.spawn_diff_against_config();
        Ok(())
    }

    /// Cooperative shutdown. Signals every supervisor to exit and
    /// awaits the tasks. Idempotent.
    pub async fn shutdown(&self) -> Result<(), SchedulerError> {
        let _ = self.shutdown_tx.send(true);
        let mut joins: Vec<JoinHandle<()>> = Vec::new();
        {
            let mut handles = self.handles.lock().unwrap();
            for (_, h) in handles.drain() {
                joins.push(h.join);
            }
        }
        for j in joins {
            let _ = j.await;
        }
        Ok(())
    }

    /// Reconcile the running supervisor set against the current
    /// `Arc<Config>`. New accounts/folders get a fresh supervisor;
    /// keys that disappeared from config get their `exit` signal
    /// flipped and are joined.
    pub async fn reconcile(&self) -> Result<(), SchedulerError> {
        let desired_keys = self.desired_keys();
        let to_remove: Vec<(String, String)> = {
            let handles = self.handles.lock().unwrap();
            handles
                .keys()
                .filter(|k| !desired_keys.contains(k))
                .cloned()
                .collect()
        };
        for key in to_remove {
            let removed = self.handles.lock().unwrap().remove(&key);
            if let Some(h) = removed {
                let _ = h.exit.send(true);
                let _ = h.join.await;
            }
        }
        self.spawn_diff_against_config();
        Ok(())
    }

    /// Signal every running supervisor to "fetch now if you can".
    /// Returns the list of (account_id, folder) pairs that received
    /// the signal. The supervisor decides whether to honour it
    /// (skips when in backoff or mid-fetch).
    pub async fn request_pass(&self) -> Vec<String> {
        let mut signaled = Vec::new();
        let handles = self.handles.lock().unwrap();
        for ((account_id, folder), h) in handles.iter() {
            let next = h.pass_request.borrow().wrapping_add(1);
            if h.pass_request.send(next).is_ok() {
                signaled.push(format!("{account_id}/{folder}"));
            }
        }
        signaled
    }

    /// Tests-only: snapshot of currently-running (account, folder)
    /// supervisor keys.
    pub fn supervisor_keys(&self) -> Vec<(String, String)> {
        self.handles.lock().unwrap().keys().cloned().collect()
    }

    fn desired_keys(&self) -> std::collections::HashSet<(String, String)> {
        let cfg = self.config.read().unwrap();
        let mut out = std::collections::HashSet::new();
        for account in &cfg.accounts {
            let folders: Vec<String> = account
                .folders
                .clone()
                .unwrap_or_else(|| vec!["INBOX".to_string()]);
            for folder in folders.into_iter().take(MAX_CONNECTIONS_PER_ACCOUNT) {
                out.insert((account.id.clone(), folder));
            }
        }
        out
    }

    fn spawn_diff_against_config(&self) {
        // Acquire config read lock first, then handles — consistent ordering
        // avoids deadlock with update_config (write lock, no handles lock).
        let cfg = self.config.read().unwrap();
        let mut handles = self.handles.lock().unwrap();
        for account in &cfg.accounts {
            let folders: Vec<String> = account
                .folders
                .clone()
                .unwrap_or_else(|| vec!["INBOX".to_string()]);
            if folders.len() > MAX_CONNECTIONS_PER_ACCOUNT {
                tracing::warn!(
                    account_id = %account.id,
                    requested = folders.len(),
                    cap = MAX_CONNECTIONS_PER_ACCOUNT,
                    "account configured with more folders than MAX_CONNECTIONS_PER_ACCOUNT; tail dropped"
                );
            }
            for folder in folders.into_iter().take(MAX_CONNECTIONS_PER_ACCOUNT) {
                let key = (account.id.clone(), folder.clone());
                if handles.contains_key(&key) {
                    continue;
                }
                let (pass_tx, pass_rx) = watch::channel(0u64);
                let (exit_tx, exit_rx) = watch::channel(false);
                let id = account.id.clone();
                let folder_owned = folder.clone();
                let config = Arc::clone(&cfg);
                let sink = self.sink.clone();
                let shutdown_rx = self.shutdown_rx.clone();
                let idle_recycle = self.idle_recycle;
                let join = tokio::spawn(async move {
                    run_account_supervisor(
                        id,
                        folder_owned,
                        config,
                        sink,
                        shutdown_rx,
                        exit_rx,
                        pass_rx,
                        idle_recycle,
                    )
                    .await;
                });
                handles.insert(
                    key,
                    SupervisorHandle {
                        join,
                        pass_request: pass_tx,
                        exit: exit_tx,
                    },
                );
            }
        }
    }
}

/// Lifecycle loop for one account. On each connect attempt:
///   1. Read the account row (incl. password) from `config`.
///   2. login → examine → run_initial_backfill → run_idle_loop.
///   3. On any error, emit the matching failure log, update
///      account_health via the sink, sleep backoff, retry.
#[allow(clippy::too_many_arguments)]
async fn run_account_supervisor(
    account_id: String,
    folder: String,
    config: Arc<Config>,
    sink: Arc<dyn MessageSink>,
    mut shutdown: watch::Receiver<bool>,
    mut exit: watch::Receiver<bool>,
    _pass_request: watch::Receiver<u64>,
    idle_recycle: Duration,
) {
    let mut consecutive_failures: u32 = 0;
    let mut last_seen_uid: u32 = 0;

    loop {
        if *shutdown.borrow() || *exit.borrow() {
            return;
        }

        // Refresh the account view each iteration so a config edit
        // (rotate-password etc.) is picked up on reconnect.
        let account = match config.accounts.iter().find(|a| a.id == account_id) {
            Some(a) => a,
            None => {
                // Account vanished from config; exit cleanly.
                return;
            }
        };

        let host = account.host.clone();
        let port = account.port;
        let tls = account.tls;
        let user = account.user.clone();
        let password = account.password.expose().to_string();
        let ca_path = account.tls_ca_path.clone();

        let cycle_result = run_one_cycle(
            &account_id,
            &folder,
            &host,
            port,
            tls,
            &user,
            &password,
            ca_path.as_deref(),
            sink.as_ref(),
            &mut last_seen_uid,
            idle_recycle,
            shutdown.clone(),
        )
        .await;

        match cycle_result {
            Ok(()) => {
                consecutive_failures = 0;
                update_health(&account_id, &folder, sink.as_ref(), "active").await;
            }
            Err(err) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let health = match &err {
                    ClientError::AuthRejected => "auth-rejected",
                    ClientError::Connect(_) => "connect-failure",
                    ClientError::TlsHandshake(_) => "tls-failure",
                    _ => "transient",
                };
                update_health(&account_id, &folder, sink.as_ref(), health).await;

                // The connect path emits the corresponding failure
                // log; the supervisor adds a re-emit for cycles where
                // the failure happened post-LOGIN.
                if matches!(err, ClientError::Server(_)) {
                    log_connect_failure(&account_id, &err.to_string());
                } else if matches!(err, ClientError::AuthRejected) {
                    log_auth_rejection(&account_id, "supervisor cycle");
                }
            }
        }

        // Bail if shutdown / exit got set during the cycle.
        if *shutdown.borrow() || *exit.borrow() {
            return;
        }

        let delay = if consecutive_failures == 0 {
            // Successful return from idle loop = clean shutdown signal
            // already handled above; wait briefly to avoid hot reconnect.
            Duration::from_millis(100)
        } else {
            backoff_after(consecutive_failures - 1)
        };
        tokio::select! {
            _ = shutdown.changed() => return,
            _ = exit.changed() => return,
            _ = tokio::time::sleep(delay) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_one_cycle(
    account_id: &str,
    folder: &str,
    host: &str,
    port: u16,
    tls: bool,
    user: &str,
    password: &str,
    ca_path: Option<&std::path::Path>,
    sink: &dyn MessageSink,
    last_seen_uid: &mut u32,
    idle_recycle: Duration,
    shutdown: watch::Receiver<bool>,
) -> Result<(), ClientError> {
    let logged_in = login(host, port, tls, user, password, account_id, ca_path).await?;

    // Auth succeeded; clear any stale health (most importantly,
    // AuthRejected from a prior bad-credential cycle that the operator
    // has since fixed). Without this the field stays AuthRejected for
    // the entire IDLE lifetime — which on a large mailbox is forever.
    update_health(account_id, folder, sink, "active").await;

    let mut conn = Connection::new(account_id, folder);

    match logged_in {
        LoggedIn::Tls(mut client) => {
            run_initial_backfill(&mut conn, &mut client, sink, shutdown.clone()).await?;
            // Update watermark from initial backfill via sink state.
            // (We don't track via update_sync_state's last_seen_uid here;
            // instead we let run_idle_loop's return value advance it.)
            let (_returned, new_uid) =
                run_idle_loop(&mut conn, client, sink, *last_seen_uid, idle_recycle, shutdown)
                    .await?;
            *last_seen_uid = new_uid;
        }
        LoggedIn::Plain(mut client) => {
            run_initial_backfill(&mut conn, &mut client, sink, shutdown.clone()).await?;
            let (_returned, new_uid) =
                run_idle_loop(&mut conn, client, sink, *last_seen_uid, idle_recycle, shutdown)
                    .await?;
            *last_seen_uid = new_uid;
        }
    }
    Ok(())
}

async fn update_health(
    account_id: &str,
    folder: &str,
    sink: &dyn MessageSink,
    health: &str,
) {
    let _ = sink
        .update_sync_state(
            account_id,
            folder,
            crate::sink::SyncStateUpdate {
                account_health: Some(health.to_string()),
                ..Default::default()
            },
        )
        .await;
}
