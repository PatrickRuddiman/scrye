//! HTTP handlers for the write/admin endpoints. Each returns 202 once
//! the underlying action has *begun*; the work itself runs asynchronously
//! in storage / search / scheduler tasks.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::response::error_response;
use crate::state::{AppState, ReindexHandle};

#[derive(Debug, Serialize)]
struct SyncStartedDto {
    started: bool,
    scope: SyncScopeDto,
}

#[derive(Debug, Serialize)]
struct SyncScopeDto {
    accounts: Vec<String>,
}

/// `POST /sync` — request an immediate sync pass against every
/// healthy supervisor. When the scheduler is wired (production),
/// returns the (account_id/folder) pairs that were actually
/// signaled. Falls back to the active-account list from storage
/// when the scheduler isn't available (api-only tests).
pub async fn handle_sync(State(state): State<AppState>) -> Response {
    let accounts = if let Some(sched) = state.scheduler.as_ref() {
        sched.request_pass().await
    } else {
        match state.storage.list_active_accounts().await {
            Ok(rows) => rows.into_iter().map(|a| a.account_id).collect::<Vec<_>>(),
            Err(e) => {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    &e.to_string(),
                );
            }
        }
    };
    (
        StatusCode::ACCEPTED,
        Json(SyncStartedDto {
            started: true,
            scope: SyncScopeDto { accounts },
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
struct ReindexStartedDto {
    started: bool,
}

/// `POST /internal/reindex` — single-flight rebuild. Truncates the
/// witchcraft DB and re-enqueues every non-tombstoned message; the
/// drainer task picks them up. A second concurrent reindex while one is
/// already in flight returns `409 conflict`.
pub async fn handle_reindex(State(state): State<AppState>) -> Response {
    let mut guard = state.reindex_lock.lock().await;
    if guard.is_some() {
        return error_response(
            StatusCode::CONFLICT,
            "conflict",
            "a reindex is already in progress",
        );
    }

    if let Err(e) = state.indexer.truncate().await {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &e.to_string(),
        );
    }
    let count = match state.storage.reenqueue_all_messages().await {
        Ok(n) => n,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            );
        }
    };

    *guard = Some(ReindexHandle {
        started_at: Instant::now(),
        messages_count: count,
    });
    drop(guard);

    // The handle stays in the lock while the drainer rebuilds. v2 will
    // clear it when the drainer notifies "queue drained"; v1 leaves it
    // forever (subsequent reindex requests during the rebuild get 409,
    // which is the desired single-flight behavior). Restarting the
    // daemon clears the in-memory lock.

    (
        StatusCode::ACCEPTED,
        Json(ReindexStartedDto { started: true }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
struct ReconcileResultDto {
    started: bool,
    accounts_added: Vec<String>,
    accounts_updated: Vec<String>,
    accounts_inactivated: Vec<String>,
}

/// `POST /internal/reconcile` — re-read config from disk (when the
/// daemon knows its config path), mirror the updated account set into
/// storage, and signal the imap-sync scheduler to reconcile its
/// supervisor tree. This is the live-reload path triggered by the CLI's
/// `add-account` / `remove-account` flow after writing `config.toml`.
pub async fn handle_reconcile(State(state): State<AppState>) -> Response {
    use scryd_config::Config;

    // If the daemon has a config path on disk, reload it now so that
    // changes written by the CLI are visible without a restart.
    if let Some(ref path) = state.config_path {
        let api_cfg = match Config::load(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    path = %path.display(),
                    err = %e,
                    "reconcile: config reload from disk failed"
                );
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "config_reload_failed",
                    "failed to reload config from disk",
                );
            }
        };
        *state.config.write().await = api_cfg;

        // Config doesn't impl Clone (password wraps SecretString), so
        // load a second copy for the scheduler — same pattern used by
        // serve_init which also loads the config twice.
        if let Some(ref sched) = state.scheduler {
            let sched_cfg = match Config::load(path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(
                        path = %path.display(),
                        err = %e,
                        "reconcile: scheduler config reload from disk failed"
                    );
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "config_reload_failed",
                        "failed to reload config from disk",
                    );
                }
            };
            sched.update_config(Arc::new(sched_cfg));
        }
    }

    let config = state.config.read().await;
    let diff = match state.storage.reconcile_from_config(&config).await {
        Ok(d) => d,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            );
        }
    };
    drop(config);

    // Reconcile the scheduler's live supervisor set so newly-added
    // accounts start being fetched immediately (fixes issue #9).
    if let Some(ref sched) = state.scheduler {
        if let Err(e) = sched.reconcile().await {
            tracing::error!(err = %e, "reconcile: scheduler reconcile failed");
        }
    }

    (
        StatusCode::ACCEPTED,
        Json(ReconcileResultDto {
            started: true,
            accounts_added: diff.added,
            accounts_updated: diff.updated,
            accounts_inactivated: diff.inactivated,
        }),
    )
        .into_response()
}
