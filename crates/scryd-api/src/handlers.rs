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

    // Caller-driven account scope. Empty = all accounts.
    let account_ids: Vec<String> = q
        .account_ids
        .as_deref()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let search_resp = match state
        .searcher
        .search(&SearchQuery {
            q: q.q.clone(),
            mode,
            k,
            account_ids: account_ids.clone(),
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
        if !filter_match(&row, &q, &account_ids, since_ts, until_ts) {
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
    account_ids: &[String],
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
    if !account_ids.is_empty() && !account_ids.iter().any(|id| id == &row.account_id) {
        return false;
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

/// `GET /status` — daemon uptime + per-account sync state. Open
/// endpoint; the consumer's higher-layer api decides who can see
/// what subset.
pub async fn handle_status(State(state): State<AppState>) -> Response {
    use serde::Serialize;
    #[derive(Serialize)]
    struct AccountStatus {
        account_id: String,
        folders: Vec<String>,
        health: Option<String>,
        last_sync_unix: Option<i64>,
        last_seen_uid: Option<u32>,
    }
    #[derive(Serialize)]
    struct LastIndexError {
        message_id: String,
        error: String,
        attempts: u32,
    }
    #[derive(Serialize)]
    struct DrainerStatus {
        queue_depth: i64,
        failed_permanent: i64,
        last_index_error: Option<LastIndexError>,
    }
    #[derive(Serialize)]
    struct DaemonStatus {
        restart_count: i64,
        consecutive_crashes: u32,
        last_crash_unix: Option<i64>,
        in_crash_loop: bool,
        in_backoff: bool,
        backoff_until_unix: Option<i64>,
    }
    #[derive(Serialize)]
    struct StatusResp {
        ok: bool,
        uptime_secs: u64,
        accounts: Vec<AccountStatus>,
        drainer: DrainerStatus,
        daemon: DaemonStatus,
    }

    let accounts = match state.storage.list_active_accounts().await {
        Ok(a) => a,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            );
        }
    };

    let mut out = Vec::new();
    for a in accounts {
        let primary_folder = a
            .folders
            .first()
            .cloned()
            .unwrap_or_else(|| "INBOX".to_string());
        let sync = state
            .storage
            .get_sync_state(&a.account_id, &primary_folder)
            .await
            .ok()
            .flatten();
        out.push(AccountStatus {
            account_id: a.account_id,
            folders: a.folders,
            health: sync.as_ref().map(|s| format!("{:?}", s.account_health)),
            last_sync_unix: sync.as_ref().and_then(|s| s.last_full_sync_at),
            last_seen_uid: sync.as_ref().map(|s| s.last_seen_uid),
        });
    }

    let queue = match state.storage.queue_health().await {
        Ok(h) => h,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            );
        }
    };

    let daemon_health = state.daemon_health;
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let in_backoff = daemon_health
        .backoff_until_unix
        .map(|until| now_unix < until)
        .unwrap_or(false);

    let drainer = DrainerStatus {
        queue_depth: queue.depth,
        failed_permanent: queue.failed_permanent,
        last_index_error: queue.last_error.map(|e| LastIndexError {
            message_id: e.message_id,
            error: e.error,
            attempts: e.attempts,
        }),
    };
    let daemon = DaemonStatus {
        restart_count: daemon_health.restart_count,
        consecutive_crashes: daemon_health.consecutive_crashes,
        last_crash_unix: daemon_health.last_crash_unix,
        in_crash_loop: daemon_health.in_crash_loop,
        in_backoff,
        backoff_until_unix: daemon_health.backoff_until_unix,
    };

    Json(StatusResp {
        // A daemon in a crash loop is not healthy even though this request
        // happened to land in an up window. Reflect the durable crash state
        // instead of the previous hardcoded `true`.
        ok: !daemon_health.in_crash_loop,
        uptime_secs: state.started_at.elapsed().as_secs(),
        accounts: out,
        drainer,
        daemon,
    })
    .into_response()
}

// Need this so Mode::from_str works in the search handler.
fn _force_mode_fromstr_inscope() -> Result<Mode, String> {
    Mode::from_str("fulltext")
}
