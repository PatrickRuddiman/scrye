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

/// Per-account state held by the scheduler.
struct SupervisorHandle {
    join: JoinHandle<()>,
}

pub struct Scheduler {
    config: Arc<Config>,
    sink: Arc<dyn MessageSink>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    handles: Mutex<HashMap<String, SupervisorHandle>>,
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
            config,
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

    /// Spawn a supervisor task per configured account. Idempotent: if
    /// a supervisor for an account id is already running, it is left
    /// alone.
    pub async fn start(&self) -> Result<(), SchedulerError> {
        let mut handles = self.handles.lock().unwrap();
        for account in &self.config.accounts {
            if handles.contains_key(&account.id) {
                continue;
            }
            let id = account.id.clone();
            let config = self.config.clone();
            let sink = self.sink.clone();
            let shutdown_rx = self.shutdown_rx.clone();
            let folder = account
                .folders
                .as_ref()
                .and_then(|f| f.first().cloned())
                .unwrap_or_else(|| "INBOX".to_string());
            let idle_recycle = self.idle_recycle;
            let join = tokio::spawn(async move {
                run_account_supervisor(id, folder, config, sink, shutdown_rx, idle_recycle).await;
            });
            handles.insert(account.id.clone(), SupervisorHandle { join });
        }
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

    /// Reconcile placeholder. v0.3.0 ships the start/shutdown lifecycle;
    /// dynamic add/remove of accounts via `reconcile` is left for a
    /// follow-up that adds the diff-and-spawn machinery.
    pub async fn reconcile(&self) -> Result<(), SchedulerError> {
        Ok(())
    }

    /// request_pass placeholder. v0.3.0 ships the IDLE-driven
    /// freshness path; on-demand "fetch now" signaling is left for
    /// the api slice's POST /sync.
    pub async fn request_pass(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Lifecycle loop for one account. On each connect attempt:
///   1. Read the account row (incl. password) from `config`.
///   2. login → examine → run_initial_backfill → run_idle_loop.
///   3. On any error, emit the matching failure log, update
///      account_health via the sink, sleep backoff, retry.
async fn run_account_supervisor(
    account_id: String,
    folder: String,
    config: Arc<Config>,
    sink: Arc<dyn MessageSink>,
    mut shutdown: watch::Receiver<bool>,
    idle_recycle: Duration,
) {
    let mut consecutive_failures: u32 = 0;
    let mut last_seen_uid: u32 = 0;

    loop {
        if *shutdown.borrow() {
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

        let cycle_result =
            run_one_cycle(&account_id, &folder, &host, port, tls, &user, &password, sink.as_ref(), &mut last_seen_uid, idle_recycle, shutdown.clone()).await;

        match cycle_result {
            Ok(()) => {
                consecutive_failures = 0;
                update_health(&sink, &account_id, &folder, "active").await;
            }
            Err(err) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let health = match &err {
                    ClientError::AuthRejected => "auth-rejected",
                    ClientError::Connect(_) => "connect-failure",
                    ClientError::TlsHandshake(_) => "tls-failure",
                    _ => "transient",
                };
                update_health(&sink, &account_id, &folder, health).await;

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

        // Bail if shutdown got set during the cycle.
        if *shutdown.borrow() {
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
    sink: &dyn MessageSink,
    last_seen_uid: &mut u32,
    idle_recycle: Duration,
    shutdown: watch::Receiver<bool>,
) -> Result<(), ClientError> {
    let logged_in = login(host, port, tls, user, password, account_id).await?;

    let mut conn = Connection::new(account_id, folder);

    match logged_in {
        LoggedIn::Tls(mut client) => {
            run_initial_backfill(&mut conn, &mut client, sink).await?;
            // Update watermark from initial backfill via sink state.
            // (We don't track via update_sync_state's last_seen_uid here;
            // instead we let run_idle_loop's return value advance it.)
            let (_returned, new_uid) =
                run_idle_loop(&mut conn, client, sink, *last_seen_uid, idle_recycle, shutdown)
                    .await?;
            *last_seen_uid = new_uid;
        }
        LoggedIn::Plain(mut client) => {
            run_initial_backfill(&mut conn, &mut client, sink).await?;
            let (_returned, new_uid) =
                run_idle_loop(&mut conn, client, sink, *last_seen_uid, idle_recycle, shutdown)
                    .await?;
            *last_seen_uid = new_uid;
        }
    }
    Ok(())
}

async fn update_health(
    sink: &Arc<dyn MessageSink>,
    account_id: &str,
    folder: &str,
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
