//! Daemon orchestration. Wires logging + preflight + storage +
//! indexer drainer + scheduler + api router into a single async
//! entry point. `scryd serve` calls [`serve()`]; tests call
//! [`serve_init`] / [`serve_run`] split so they can drive a
//! controlled shutdown.

use std::sync::Arc;

use scryd_api::{bind, router as api_router, AppState};
use scryd_config::Config;
use scryd_imap::{MessageSink, Scheduler};
use scryd_log::{kind, log_lifecycle};
use scryd_search::{Drainer, InMemoryIndexer};
use scryd_storage::StorageHandle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::sink::StorageMessageSink;
use crate::xdg::{config_path, data_dir, runtime_dir};
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

    let data = data_dir()?;
    let scryd_data = data.join("scryd");
    std::fs::create_dir_all(&scryd_data).ok();
    let storage = StorageHandle::open(&scryd_data, 4)?;

    let indexer: Arc<dyn scryd_search::Indexer> = Arc::new(InMemoryIndexer::new());

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

    let app_state = AppState::new(storage, indexer, api_config);
    let router = api_router(app_state);

    let runtime = runtime_dir()?;
    let listener = bind(&runtime).await.map_err(|e| {
        RuntimeError::PermissionInvariant {
            path: runtime,
            reason: e.to_string(),
        }
    })?;

    let (api_tx, api_rx) = tokio::sync::watch::channel(false);
    let api_handle = tokio::spawn(scryd_api::serve(listener, router, async move {
        let mut rx = api_rx;
        let _ = rx.changed().await;
    }));

    log_lifecycle!(kind = kind::STARTUP);

    Ok(ServeContext {
        scheduler,
        drainer_handle,
        drainer_shutdown,
        api_handle,
        api_shutdown: api_tx,
    })
}

/// Wait for SIGTERM/SIGINT, then cooperatively tear down api router
/// → scheduler → drainer → storage. Idempotent for the
/// already-shut-down case.
pub async fn serve_run(ctx: ServeContext) -> Result<(), RuntimeError> {
    wait_for_shutdown_signal().await;

    // Stop accepting new api connections and let in-flight ones
    // drain.
    let _ = ctx.api_shutdown.send(true);
    let _ = ctx.api_handle.await;

    let _ = ctx.scheduler.shutdown().await;

    ctx.drainer_shutdown.cancel();
    let _ = ctx.drainer_handle.await;

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
