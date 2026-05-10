//! axum router. Mounts the public read endpoints from task 18 and the
//! request-access-log middleware that emits a `kind::REQUEST` log line
//! per response.

use std::time::Instant;

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use scryd_log::{kind, log_failure, log_request};

use crate::state::AppState;
use crate::{handlers, handlers_write};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/search", get(handlers::handle_search))
        .route("/message/:id", get(handlers::handle_message))
        .route("/message/:id/raw", get(handlers::handle_raw))
        .route("/thread/:id", get(handlers::handle_thread))
        .route("/accounts", get(handlers::handle_accounts))
        .route("/status", get(handlers::handle_status))
        .route("/sync", post(handlers_write::handle_sync))
        .route("/internal/reindex", post(handlers_write::handle_reindex))
        .route(
            "/internal/reconcile",
            post(handlers_write::handle_reconcile),
        )
        .layer(middleware::from_fn(access_log))
        .with_state(state)
}

/// Emit a `kind::REQUEST` access-log event at DEBUG for 2xx responses; at
/// WARN for 4xx; at ERROR for 5xx (per observability slice §3 Decision 6).
async fn access_log(req: Request<Body>, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let started = Instant::now();
    let response = next.run(req).await;
    let duration_ms = started.elapsed().as_millis() as u64;
    let status = response.status();
    let status_code = status.as_u16();

    if status.is_success() || status == StatusCode::ACCEPTED {
        log_request!(
            method = %method,
            path = %path,
            status = status_code,
            duration_ms = duration_ms
        );
    } else if status.is_client_error() {
        log_failure!(
            severity = warn,
            kind = kind::REQUEST,
            method = %method,
            path = %path,
            status = status_code,
            duration_ms = duration_ms
        );
    } else if status.is_server_error() {
        log_failure!(
            kind = kind::REQUEST,
            method = %method,
            path = %path,
            status = status_code,
            duration_ms = duration_ms
        );
    }

    response
}
