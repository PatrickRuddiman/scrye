//! axum router scaffolding. Tasks 18–19 mount handlers on this router.
//! The peercred guard would normally be a tower layer, but the simplest
//! correct enforcement is at accept time in `serve.rs` — that's where
//! [`crate::peercred::check_stream_peer`] runs before any axum code.

use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new().with_state(state)
}
