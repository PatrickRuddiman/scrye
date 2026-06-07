//! Daemon orchestration. Wires logging + preflight + storage +
//! indexer drainer + scheduler + api router into a single async
//! entry point. `scryd serve` calls [`serve()`]; tests call
//! [`serve_init`] / [`serve_run`] split so they can drive a
//! controlled shutdown.

use std::sync::Arc;
use std::time::Duration;

use scryd_api::{bind, router as api_router, AppState};
use scryd_config::Config;
use scryd_imap::{MessageSink, Scheduler};
use scryd_log::{kind, log_lifecycle};
use scryd_search::{Drainer, WitchcraftIndexer};
use scryd_storage::StorageHandle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::sink::StorageMessageSink;
use crate::xdg::{assets_dir, config_path, data_dir, runtime_dir};
use crate::RuntimeError;

/// Components started by [`serve_init`]. The api router has already
/// been bound to a UDS listener and is being served; the scheduler
/// is running its supervisors. [`serve_run`] awaits SIGTERM/SIGINT
/// and shuts everything down cleanly.
pub struct ServeContext {
    scheduler: Arc<Scheduler>,
    drainer_handle: JoinHandle<()>,
    drainer_shutdown: tokio_util::sync::CancellationToken,
    api_handle: JoinHandle<Result<(), scryd_api::ApiError>>,
    api_shutdown: tokio::sync::watch::Sender<bool>,
}

/// Entry point for the daemon. Boot logging + preflight + storage +
/// indexer drainer + scheduler + api router, then wait for SIGTERM
/// / SIGINT, then cooperatively tear everything down.
pub async fn serve() -> Result<(), RuntimeError> {
    let ctx = serve_init().await?;
    serve_run(ctx).await
}

/// Boot every component the daemon owns and return handles to them
/// so `serve_run` (or a test) can drive shutdown.
pub async fn serve_init() -> Result<ServeContext, RuntimeError> {
    install_panic_hook();

    let _ = scryd_log::init();

    crate::preflight::run()?;

    let cfg_path = config_path()?;
    // Load twice: once for the scheduler (Arc<Config>), once for the
    // axum AppState (owned). Config doesn't impl Clone (AccountPassword
    // wraps SecretString). The two reads happen back-to-back; any
    // inconsistency window is microseconds.
    let scheduler_config = Arc::new(Config::load(&cfg_path)?);
    let api_config = Config::load(&cfg_path)?;

    let scryd_data = data_dir()?;
    std::fs::create_dir_all(&scryd_data).ok();
    let storage = StorageHandle::open(&scryd_data, 4)?;

    // Seed `accounts` rows from the current config so the FK on
    // `messages.account_id` doesn't reject the first batch of fetches.
    let _ = storage.reconcile_from_config(&api_config).await?;

    // Witchcraft is the production indexer. The sqlite file at
    // <data_dir>/witchcraft.sqlite is the persistence boundary —
    // deleting it resets the index. The three asset files witchcraft's
    // t5-quantized backend loads (config.json, tokenizer.json,
    // xtr.gguf) live under <assets_dir>; scryd-fetch-weights downloads
    // + extracts the bundle on first start when any are missing.
    let assets = assets_dir()?;
    std::fs::create_dir_all(&assets).ok();
    let witchcraft_db = scryd_data.join("witchcraft.sqlite");
    ensure_weights_present(&assets).await?;
    let indexer: Arc<dyn scryd_search::Indexer> = Arc::new(
        WitchcraftIndexer::open(&witchcraft_db, &assets)
            .await
            .map_err(|e| RuntimeError::PermissionInvariant {
                path: witchcraft_db.clone(),
                reason: format!("witchcraft open: {e}"),
            })?,
    );

    let drainer = Drainer::new(storage.clone(), indexer.clone());
    let drainer_notify: Arc<Notify> = drainer.enqueue_notify();
    let drainer_shutdown = drainer.shutdown_token();
    let drainer_handle = tokio::spawn(drainer.run());

    let sink_concrete = Arc::new(StorageMessageSink::new(
        storage.clone(),
        drainer_notify,
        scryd_data.clone(),
    ));
    let sink: Arc<dyn MessageSink> = sink_concrete;

    let scheduler = Arc::new(Scheduler::new(scheduler_config, sink));
    scheduler.start().await.map_err(|e| {
        RuntimeError::PermissionInvariant {
            path: cfg_path.clone(),
            reason: format!("scheduler start: {e}"),
        }
    })?;

    let require_peer_uid = api_config.server.require_peer_uid;
    let socket_mode = api_config.server.socket_mode;
    let app_state = AppState::with_scheduler(
        storage,
        indexer,
        api_config,
        Some(scheduler.clone()),
        Some(cfg_path),
    );
    let router = api_router(app_state);

    let runtime = runtime_dir()?;
    let listener = bind(&runtime, socket_mode).await.map_err(|e| {
        RuntimeError::PermissionInvariant {
            path: runtime,
            reason: e.to_string(),
        }
    })?;

    let (api_tx, api_rx) = tokio::sync::watch::channel(false);
    let api_handle = tokio::spawn(scryd_api::serve(
        listener,
        router,
        require_peer_uid,
        async move {
            let mut rx = api_rx;
            let _ = rx.changed().await;
        },
    ));

    log_lifecycle!(kind = kind::STARTUP);

    Ok(ServeContext {
        scheduler,
        drainer_handle,
        drainer_shutdown,
        api_handle,
        api_shutdown: api_tx,
    })
}

/// Grace period for the API handler to finish in-flight requests after
/// the shutdown signal is sent. UDS JSON requests are short round-trips;
/// 5 s is generous and keeps the total maximum shutdown time well under
/// systemd's `TimeoutStopSec=90s`.
const SHUTDOWN_API_GRACE: Duration = Duration::from_secs(5);

/// Grace period for IMAP supervisors to exit after the shutdown watch
/// fires. Supervisors check the signal between backfill batches and in
/// the IDLE select loop, so they exit within one batch-time. 40 s covers
/// the worst-case single-batch on a large mailbox.
const SHUTDOWN_SCHEDULER_GRACE: Duration = Duration::from_secs(40);

/// Grace period for the drainer task to exit after its cancellation
/// token fires. The drainer now checks the token before every
/// flush_embeddings / flush_index call, so it exits within one
/// in-flight spawn_blocking call's latency (≤ ~20 s). 30 s is a
/// conservative ceiling for the case where an embed call was already
/// dispatched before the cancel was observed.
///
/// Total maximum shutdown wall-time:
///   SHUTDOWN_API_GRACE (5) + SHUTDOWN_SCHEDULER_GRACE (40)
///   + SHUTDOWN_DRAINER_GRACE (30) = 75 s < 90 s.
const SHUTDOWN_DRAINER_GRACE: Duration = Duration::from_secs(30);

/// Wait for SIGTERM/SIGINT, then cooperatively tear down api router
/// → scheduler → drainer → storage. Each phase has an explicit upper
/// bound so the total shutdown wall-time stays under systemd's
/// `TimeoutStopSec=90s` even under heavy indexing load (issue #13).
pub async fn serve_run(ctx: ServeContext) -> Result<(), RuntimeError> {
    wait_for_shutdown_signal().await;

    // Phase 1 — stop accepting new API connections. In-flight UDS
    // requests are small JSON round-trips; SHUTDOWN_API_GRACE is
    // generous for them to drain.
    let _ = ctx.api_shutdown.send(true);
    if tokio::time::timeout(SHUTDOWN_API_GRACE, ctx.api_handle)
        .await
        .is_err()
    {
        warn!(
            target: "scryd_runtime::serve",
            "api handler did not exit within grace period; proceeding"
        );
    }

    // Phase 2 — stop the IMAP scheduler. Supervisors check the
    // shutdown watch between backfill batches and race it in the IDLE
    // select, so they exit quickly; the timeout guards against a
    // stalled or very-slow-backfilling session.
    if tokio::time::timeout(SHUTDOWN_SCHEDULER_GRACE, ctx.scheduler.shutdown())
        .await
        .is_err()
    {
        warn!(
            target: "scryd_runtime::serve",
            "scheduler did not exit within grace period; proceeding"
        );
    }

    // Phase 3 — stop the drainer. Cancel first so the loop exits as
    // soon as the current in-flight operation finishes; the timeout
    // guards against a flush_index cascade that started before the
    // cancel was observed.
    ctx.drainer_shutdown.cancel();
    if tokio::time::timeout(SHUTDOWN_DRAINER_GRACE, ctx.drainer_handle)
        .await
        .is_err()
    {
        warn!(
            target: "scryd_runtime::serve",
            "drainer did not exit within grace period; proceeding"
        );
    }

    log_lifecycle!(kind = kind::SHUTDOWN);
    Ok(())
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut sigint = match signal(SignalKind::interrupt()) {
        Ok(s) => s,
        Err(_) => return,
    };
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Files witchcraft's `t5-quantized` backend loads from the assets
/// dir. scryd-fetch-weights downloads + extracts the bundle that
/// contains all three; this check just verifies what's on disk
/// afterwards.
const REQUIRED_WITCHCRAFT_ASSETS: &[&str] = &[
    "config.json",
    "tokenizer.json",
    "xtr.gguf",
];

/// Ensure the witchcraft asset bundle is on disk before opening
/// the indexer. Calls `scryd-fetch-weights` to download + extract
/// when any of the four required files is missing.
async fn ensure_weights_present(assets: &std::path::Path) -> Result<(), RuntimeError> {
    if REQUIRED_WITCHCRAFT_ASSETS
        .iter()
        .all(|f| assets.join(f).exists())
    {
        return Ok(());
    }
    auto_fetch_weights(assets).await?;
    // Sanity check: after fetch the four files must exist.
    let missing: Vec<&str> = REQUIRED_WITCHCRAFT_ASSETS
        .iter()
        .copied()
        .filter(|f| !assets.join(f).exists())
        .collect();
    if !missing.is_empty() {
        return Err(RuntimeError::PermissionInvariant {
            path: assets.to_path_buf(),
            reason: format!("fetcher succeeded but assets still missing: {missing:?}"),
        });
    }
    Ok(())
}

#[cfg(unix)]
async fn auto_fetch_weights(assets: &std::path::Path) -> Result<(), RuntimeError> {
    log_lifecycle!(
        kind = kind::STARTUP,
        info = "fetching xtr-int4 asset bundle on first start"
    );
    let bin = std::env::var("SCRYD_FETCH_WEIGHTS_BIN")
        .unwrap_or_else(|_| "/usr/local/bin/scryd-fetch-weights".to_string());
    let assets_owned = assets.to_path_buf();
    let bin_owned = bin.clone();
    let status = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&bin_owned)
            .arg("--target")
            .arg(&assets_owned)
            .status()
    })
    .await
    .map_err(|e| RuntimeError::PermissionInvariant {
        path: assets.to_path_buf(),
        reason: format!("auto-fetch join: {e}"),
    })?
    .map_err(|e| RuntimeError::PermissionInvariant {
        path: assets.to_path_buf(),
        reason: format!("invoke {bin}: {e}"),
    })?;
    if !status.success() {
        return Err(RuntimeError::PermissionInvariant {
            path: assets.to_path_buf(),
            reason: format!("{bin} exited {status}"),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
async fn auto_fetch_weights(_assets: &std::path::Path) -> Result<(), RuntimeError> {
    Ok(())
}

fn install_panic_hook() {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("scryd: panic: {info}");
            std::process::abort();
        }));
    });
}
