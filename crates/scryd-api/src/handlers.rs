//! HTTP handlers for the public read endpoints. Tasks 18 ships these;
//! task 19 layers writes/admin (`/sync`, `/internal/reindex`,
//! `/internal/reconcile`) on top.

use std::str::FromStr;
use std::time::Instant;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use scryd_search::{Mode, SearchQuery, K_MIN, K_MULTIPLIER};

use crate::dto::{
    AccountDto, AccountsResponseDto, SearchHitDto, SearchQueryDto, SearchResponseDto,
    ThreadResponseDto, LIMIT_DEFAULT, LIMIT_MAX,
};
use crate::response::{error_response, iso8601_from_unix, message_row_to_dto, parse_iso_date};
use crate::state::AppState;

/// `GET /search` — ranked hits with optional filters and snippet.
pub async fn handle_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQueryDto>,
) -> Response {
    let started = Instant::now();

    let mode = match q.mode.as_deref().unwrap_or("fulltext").parse::<Mode>() {
        Ok(m) => m,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "bad_query",
                "mode must be one of fulltext|semantic|hybrid",
            );
        }
    };

    let since_ts = match q.since.as_deref().map(parse_iso_date) {
        Some(Some(ts)) => Some(ts),
        Some(None) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "bad_query",
                "since must be YYYY-MM-DD",
            );
        }
        None => None,
    };
    let until_ts = match q.until.as_deref().map(parse_iso_date) {
        Some(Some(ts)) => Some(ts + 86_400 - 1), // inclusive end-of-day
        Some(None) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "bad_query",
                "until must be YYYY-MM-DD",
            );
        }
        None => None,
    };

    let limit = q.limit.unwrap_or(LIMIT_DEFAULT).clamp(1, LIMIT_MAX);
    let k = std::cmp::max(limit * K_MULTIPLIER, K_MIN);

    let search_resp = match state
        .searcher
        .search(&SearchQuery {
            q: q.q.clone(),
            mode,
            k,
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            );
        }
    };

    // Pull each hit's metadata from storage; apply filters; trim to limit.
    let mut hits: Vec<SearchHitDto> = Vec::new();
    for hit in search_resp.hits.into_iter() {
        let row = match state.storage.get_message(hit.message_id.as_str()).await {
            Ok(Some(r)) if r.tombstoned_at.is_none() => r,
            _ => continue,
        };
        if !filter_match(&row, &q, since_ts, until_ts) {
            continue;
        }
        let snippet = derive_snippet(&row.body_md, &hit.semantic_snippet, &q.q, mode);
        hits.push(SearchHitDto {
            message_id: row.message_id,
            account_id: row.account_id,
            folder: row.folder,
            sender_addr: row.sender_addr,
            sender_name: row.sender_name,
            subject: row.subject,
            date: iso8601_from_unix(row.date_unix),
            score: hit.score,
            snippet: Some(snippet),
            thread_id: row.thread_id,
        });
        if hits.len() >= limit {
            break;
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    Json(SearchResponseDto {
        hits,
        mode: mode.as_str().to_string(),
        elapsed_ms,
    })
    .into_response()
}

fn filter_match(
    row: &scryd_storage::MessageRow,
    q: &SearchQueryDto,
    since_ts: Option<i64>,
    until_ts: Option<i64>,
) -> bool {
    if let Some(from) = q.from.as_deref() {
        if !row.sender_addr.to_lowercase().contains(&from.to_lowercase()) {
            return false;
        }
    }
    if let Some(folder) = q.folder.as_deref() {
        if row.folder != folder {
            return false;
        }
    }
    if let Some(account) = q.account.as_deref() {
        if row.account_id != account {
            return false;
        }
    }
    if let Some(s) = since_ts {
        if row.date_unix < s {
            return false;
        }
    }
    if let Some(u) = until_ts {
        if row.date_unix > u {
            return false;
        }
    }
    true
}

fn derive_snippet(
    body_md: &str,
    semantic_snippet: &Option<String>,
    q: &str,
    mode: Mode,
) -> String {
    match mode {
        Mode::Semantic => semantic_snippet.clone().unwrap_or_else(|| {
            scryd_search::snippet::render(body_md, q, 240)
        }),
        Mode::FullText | Mode::Hybrid => scryd_search::snippet::render(body_md, q, 240),
    }
}

/// `GET /message/:id` — full parsed-message shape.
pub async fn handle_message(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.storage.get_message(&id).await {
        Ok(Some(row)) if row.tombstoned_at.is_none() => Json(message_row_to_dto(row)).into_response(),
        Ok(_) => error_response(StatusCode::NOT_FOUND, "not_found", "no such message"),
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &e.to_string(),
        ),
    }
}

/// `GET /message/:id/raw` — raw RFC 5322 bytes streamed back.
pub async fn handle_raw(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let row = match state.storage.get_message(&id).await {
        Ok(Some(r)) if r.tombstoned_at.is_none() => r,
        Ok(_) => {
            return error_response(StatusCode::NOT_FOUND, "not_found", "no such message");
        }
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            );
        }
    };
    match tokio::fs::read(&row.raw_path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "message/rfc822")],
            bytes,
        )
            .into_response(),
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &e.to_string(),
        ),
    }
}

/// `GET /thread/:id` — every non-tombstoned message in the thread, oldest first.
pub async fn handle_thread(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.storage.get_thread(&id).await {
        Ok(rows) if !rows.is_empty() => {
            let messages = rows.into_iter().map(message_row_to_dto).collect();
            Json(ThreadResponseDto {
                thread_id: id,
                messages,
            })
            .into_response()
        }
        Ok(_) => error_response(StatusCode::NOT_FOUND, "not_found", "no such thread"),
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &e.to_string(),
        ),
    }
}

/// `GET /accounts` — list of configured accounts (no credentials).
pub async fn handle_accounts(State(state): State<AppState>) -> Response {
    match state.storage.list_active_accounts().await {
        Ok(rows) => {
            let accounts = rows
                .into_iter()
                .map(|a| AccountDto {
                    account_id: a.account_id,
                    folders: a.folders,
                })
                .collect();
            Json(AccountsResponseDto { accounts }).into_response()
        }
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &e.to_string(),
        ),
    }
}

// Need this so Mode::from_str works in the search handler.
fn _force_mode_fromstr_inscope() -> Result<Mode, String> {
    Mode::from_str("fulltext")
}
