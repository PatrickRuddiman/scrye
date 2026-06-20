//! Daemon orchestration. Wires logging + preflight + storage +
//! indexer drainer + scheduler + MCP server into a single async
//! entry point. The `scryd` daemon binary calls [`serve()`]; tests
//! call [`serve_init`] / [`serve_run`] split so they can drive a
//! controlled shutdown.

use std::sync::Arc;
use std::time::Duration;

use scryd_config::Config;
use scryd_imap::{MessageSink, Scheduler};
use scryd_log::{kind, log_lifecycle};
use scryd_mcp::{serve as mcp_serve, AccountScope, DaemonHealthSnapshot, McpState};
use scryd_search::{Drainer, WitchcraftIndexer};
use scryd_storage::{DaemonRunStats, StorageHandle};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::sink::StorageMessageSink;
use crate::xdg::{assets_dir, config_path, data_dir};
use crate::RuntimeError;

/// Components started by [`serve_init`]. The MCP server has already
/// been bound to its loopback TCP listener and is being served; the
/// scheduler is running its supervisors. [`serve_run`] awaits
/// SIGTERM/SIGINT and shuts everything down cleanly.
pub struct ServeContext {
    scheduler: Arc<Scheduler>,
    drainer_handle: JoinHandle<()>,
    drainer_shutdown: CancellationToken,
    mcp_handle: JoinHandle<std::io::Result<()>>,
    mcp_shutdown: CancellationToken,
    /// Handle + run id used by [`serve_run`] to mark this run cleanly
    /// stopped (`daemon_runs.clean = 1`) so the next process doesn't count
    /// it as a crash.
    storage: StorageHandle,
    run_id: i64,
    started_at: std::time::Instant,
}

/// Entry point for the daemon. Boot logging + preflight + storage +
/// indexer drainer + scheduler + MCP server, then wait for SIGTERM
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

    // USER_EMAIL is the mandatory ownership boundary. Resolve it first so a
    // missing/empty value fails fast before we touch storage or the network —
    // the daemon must never run unscoped.
    let scope = AccountScope::from_env().map_err(|_| RuntimeError::UserEmail)?;

    let cfg_path = config_path()?;
    // Partition by email: the daemon fetches, indexes, and serves exactly the
    // account(s) whose IMAP login matches USER_EMAIL. Drop every other
    // configured account up front so the scheduler never connects to them and
    // storage only ever mirrors the owned mailbox.
    let mut cfg = Config::load(&cfg_path)?;
    let configured_accounts = cfg.accounts.len();
    cfg.accounts
        .retain(|a| a.user.eq_ignore_ascii_case(scope.email()));
    if cfg.accounts.is_empty() {
        log_lifecycle!(
            severity = warn,
            kind = kind::STARTUP,
            info = "no configured account matches USER_EMAIL; daemon will idle until one is added",
            user_email = scope.email(),
            configured_accounts = configured_accounts,
        );
    }
    let mcp_bind = resolve_mcp_bind(&cfg)?;

    let scryd_data = data_dir()?;
    std::fs::create_dir_all(&scryd_data).ok();
    let storage = StorageHandle::open(&scryd_data, 4)?;

    // Record this run before touching anything that can panic/abort. A row
    // whose `stopped_at` stays NULL means the previous process died without a
    // clean shutdown; `begin_run` returns the trailing crash streak so we can
    // detect a crash loop and rate-limit it.
    let run_stats = storage
        .begin_run(env!("CARGO_PKG_VERSION"), std::process::id() as i64)
        .await?;

    // Crash-loop backoff: sleep *before* opening the heavy indexer or spawning
    // the drainer, so this throttles every crash site (indexer open AND drain),
    // not just the drainer. Interruptible by SIGTERM/SIGINT so a stuck daemon
    // can still be stopped promptly during a long backoff.
    let now_unix = now_unix_secs();
    let backoff = crash_backoff(&run_stats, now_unix);
    let backoff_until_unix = if backoff.is_zero() {
        None
    } else {
        Some(now_unix + backoff.as_secs() as i64)
    };
    if !backoff.is_zero() {
        log_lifecycle!(
            severity = warn,
            kind = kind::STARTUP,
            info = "crash-loop detected; backing off before resuming",
            consecutive_crashes = run_stats.consecutive_unclean,
            backoff_secs = backoff.as_secs(),
            last_crash_unix = ?run_stats.last_crash_unix,
        );
        let interrupted = tokio::select! {
            _ = tokio::time::sleep(backoff) => false,
            _ = wait_for_shutdown_signal() => true,
        };
        if interrupted {
            // Stopped during backoff: mark the run clean so this aborted-early
            // start doesn't inflate the next process's crash streak.
            let _ = storage.finish_run(run_stats.run_id).await;
            log_lifecycle!(
                kind = kind::SHUTDOWN,
                info = "interrupted during crash-loop backoff",
                clean_close = true,
            );
            std::process::exit(0);
        }
    }

    let daemon_health = DaemonHealthSnapshot {
        restart_count: run_stats.run_id,
        consecutive_crashes: run_stats.consecutive_unclean,
        last_crash_unix: run_stats.last_crash_unix,
        in_crash_loop: !backoff.is_zero(),
        backoff_until_unix,
    };

    // Seed `accounts` rows from the (email-filtered) config so the FK on
    // `messages.account_id` doesn't reject the first batch of fetches.
    let _ = storage.reconcile_from_config(&cfg).await?;

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

    let scheduler = Arc::new(Scheduler::new(Arc::new(cfg), sink));
    scheduler.start().await.map_err(|e| {
        RuntimeError::PermissionInvariant {
            path: cfg_path.clone(),
            reason: format!("scheduler start: {e}"),
        }
    })?;

    // The MCP server is the daemon's sole external surface: six read/search
    // tools over Streamable HTTP on a loopback TCP listener, every response
    // scoped to USER_EMAIL's account(s). Fetch + index run automatically via
    // the scheduler + drainer above; there is no write/control surface.
    let mut mcp_state = McpState::new(storage.clone(), indexer, scope);
    mcp_state.daemon_health = daemon_health;
    let started_at = mcp_state.started_at;

    let mcp_shutdown = CancellationToken::new();
    let mcp_handle = {
        let token = mcp_shutdown.clone();
        tokio::spawn(mcp_serve(mcp_state, mcp_bind, async move {
            token.cancelled().await;
        }))
    };

    log_lifecycle!(
        kind = kind::STARTUP,
        info = "mcp server listening",
        mcp_bind = %mcp_bind,
    );

    Ok(ServeContext {
        scheduler,
        drainer_handle,
        drainer_shutdown,
        mcp_handle,
        mcp_shutdown,
        storage,
        run_id: run_stats.run_id,
        started_at,
    })
}

/// Wait for SIGTERM/SIGINT, then cooperatively tear down MCP server
/// → scheduler → drainer → storage. Idempotent for the
/// already-shut-down case.
pub async fn serve_run(ctx: ServeContext) -> Result<(), RuntimeError> {
    wait_for_shutdown_signal().await;

    // Stop accepting new MCP connections and let in-flight ones drain.
    ctx.mcp_shutdown.cancel();
    let _ = ctx.mcp_handle.await;

    let _ = ctx.scheduler.shutdown().await;

    ctx.drainer_shutdown.cancel();
    let _ = ctx.drainer_handle.await;

    // Mark this run cleanly stopped (`daemon_runs.clean = 1`) so the next
    // process does not count it toward the crash streak.
    let _ = ctx.storage.finish_run(ctx.run_id).await;

    log_lifecycle!(
        kind = kind::SHUTDOWN,
        uptime_seconds = ctx.started_at.elapsed().as_secs(),
        clean_close = true,
    );
    Ok(())
}

/// Resolve the loopback TCP address the MCP server binds to. `SCRYD_MCP_BIND`
/// overrides the config `[server] mcp_bind` (default `127.0.0.1:7878`). The
/// address must parse and must be loopback — the MCP surface is local-only by
/// design (the email scope is the only auth boundary; never expose it on a
/// routable interface).
fn resolve_mcp_bind(cfg: &Config) -> Result<std::net::SocketAddr, RuntimeError> {
    let raw = std::env::var("SCRYD_MCP_BIND")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| cfg.server.mcp_bind.clone());
    let addr: std::net::SocketAddr = raw.trim().parse().map_err(|e| {
        RuntimeError::PermissionInvariant {
            path: std::path::PathBuf::from("SCRYD_MCP_BIND"),
            reason: format!("invalid mcp bind address `{raw}`: {e}"),
        }
    })?;
    if !addr.ip().is_loopback() {
        return Err(RuntimeError::PermissionInvariant {
            path: std::path::PathBuf::from("SCRYD_MCP_BIND"),
            reason: format!(
                "mcp bind address {addr} is not loopback; refusing to expose the MCP surface off-host"
            ),
        });
    }
    Ok(addr)
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

/// Consecutive unclean shutdowns before startup backoff kicks in. The first
/// few crashes restart immediately (transient faults clear themselves); only a
/// sustained loop is throttled.
const CRASH_LOOP_THRESHOLD: u32 = 3;
/// Base backoff once the threshold is crossed; doubles per extra crash.
const CRASH_BACKOFF_BASE: Duration = Duration::from_secs(5);
/// Ceiling for the exponential backoff so the daemon keeps retrying.
const CRASH_BACKOFF_CAP: Duration = Duration::from_secs(300);
/// Only treat recent crashes as a loop. An old unclean run outside this window
/// (e.g. a clean box that crashed once weeks ago) does not delay startup.
const CRASH_WINDOW: Duration = Duration::from_secs(600);

/// Seconds since the Unix epoch, saturating to 0 before 1970 (never happens in
/// practice; keeps the function total).
fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Decide how long to sleep at startup given the preceding crash history.
///
/// Returns [`Duration::ZERO`] unless there have been at least
/// [`CRASH_LOOP_THRESHOLD`] consecutive unclean shutdowns *and* the most recent
/// one is within [`CRASH_WINDOW`] of `now_unix`. Otherwise it grows the delay
/// exponentially from [`CRASH_BACKOFF_BASE`], capped at [`CRASH_BACKOFF_CAP`].
fn crash_backoff(stats: &DaemonRunStats, now_unix: i64) -> Duration {
    if stats.consecutive_unclean < CRASH_LOOP_THRESHOLD {
        return Duration::ZERO;
    }
    let recent = match stats.last_crash_unix {
        Some(last) => now_unix.saturating_sub(last) <= CRASH_WINDOW.as_secs() as i64,
        None => false,
    };
    if !recent {
        return Duration::ZERO;
    }
    // Cap the shift well below 64 so `1 << exp` can't overflow even after a
    // long loop (consecutive_unclean is bounded by KEEP_RUNS but be defensive).
    let exp = (stats.consecutive_unclean - CRASH_LOOP_THRESHOLD).min(16);
    let secs = CRASH_BACKOFF_BASE
        .as_secs()
        .saturating_mul(1u64 << exp)
        .min(CRASH_BACKOFF_CAP.as_secs());
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(consecutive_unclean: u32, last_crash_unix: Option<i64>) -> DaemonRunStats {
        DaemonRunStats {
            run_id: 1,
            consecutive_unclean,
            last_crash_unix,
        }
    }

    #[test]
    fn no_backoff_below_threshold() {
        let now = 1_000_000;
        assert_eq!(crash_backoff(&stats(0, None), now), Duration::ZERO);
        assert_eq!(crash_backoff(&stats(2, Some(now)), now), Duration::ZERO);
    }

    #[test]
    fn backoff_grows_exponentially_from_threshold() {
        let now = 1_000_000;
        // At the threshold: base (5s * 2^0).
        assert_eq!(crash_backoff(&stats(3, Some(now)), now), CRASH_BACKOFF_BASE);
        // One past: 5s * 2^1 = 10s.
        assert_eq!(
            crash_backoff(&stats(4, Some(now)), now),
            Duration::from_secs(10)
        );
        // Two past: 5s * 2^2 = 20s.
        assert_eq!(
            crash_backoff(&stats(5, Some(now)), now),
            Duration::from_secs(20)
        );
    }

    #[test]
    fn backoff_is_capped() {
        let now = 1_000_000;
        // A long loop saturates at the cap rather than overflowing.
        assert_eq!(
            crash_backoff(&stats(100, Some(now)), now),
            CRASH_BACKOFF_CAP
        );
    }

    #[test]
    fn old_crash_outside_window_does_not_back_off() {
        let now = 2_000_000;
        let stale = now - CRASH_WINDOW.as_secs() as i64 - 1;
        assert_eq!(crash_backoff(&stats(9, Some(stale)), now), Duration::ZERO);
    }

    #[test]
    fn missing_last_crash_does_not_back_off() {
        let now = 1_000_000;
        assert_eq!(crash_backoff(&stats(9, None), now), Duration::ZERO);
    }
}
