//! The six read tools as pure, scope-enforcing free functions.
//!
//! Each returns a typed DTO (or an [`McpError`]) so integration tests can
//! assert on the result directly; `server.rs`'s thin `#[tool]` wrappers
//! serialize the `Ok` DTO into MCP content. The logic is ported from the
//! retired `scryd-api`'s `handlers.rs`, with the `USER_EMAIL` scope forced in
//! at every boundary:
//!
//! - `search`: the allowed account ids are intersected with any caller filter
//!   ([`scope::narrow`]) and pushed into both the query and the post-filter;
//!   an empty scoped set short-circuits to empty results (never "all").
//! - `get_message` / `get_raw_message`: a row whose `account_id` is not owned
//!   returns `not_found`, indistinguishable from a missing id (no existence
//!   leak).
//! - `get_thread`: messages from non-owned accounts are dropped; an empty
//!   thread after filtering returns `not_found`.
//! - `list_accounts` / `status`: only owned accounts are surfaced.

use std::time::Instant;

use rmcp::ErrorData as McpError;
use scryd_search::{Mode, SearchQuery, K_MIN, K_MULTIPLIER};

use crate::dto::{
    derive_snippet, filter_match, iso8601_from_unix, message_row_to_dto, AccountDto,
    AccountStatusDto, AccountsResponseDto, DaemonStatusDto, DrainerStatusDto, IdArg,
    LastIndexErrorDto, MessageDto, RawMessageDto, SearchArgs, SearchHitDto, SearchResponseDto,
    StatusResponseDto, ThreadResponseDto, LIMIT_DEFAULT, LIMIT_MAX, SNIPPET_CHARS,
};
use crate::scope::{self, retain_allowed};
use crate::state::McpState;

fn internal_error(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

fn not_found(message: &'static str) -> McpError {
    McpError::resource_not_found(message, None)
}

/// `search` — ranked hits scoped to `USER_EMAIL`'s account(s).
pub async fn search(state: &McpState, args: SearchArgs) -> Result<SearchResponseDto, McpError> {
    let started = Instant::now();

    let mode = match args.mode.as_deref().unwrap_or("fulltext").parse::<Mode>() {
        Ok(m) => m,
        Err(_) => {
            return Err(McpError::invalid_params(
                "mode must be one of fulltext|semantic|hybrid",
                None,
            ));
        }
    };

    let since_ts = match args.since.as_deref().map(crate::dto::parse_iso_date) {
        Some(Some(ts)) => Some(ts),
        Some(None) => return Err(McpError::invalid_params("since must be YYYY-MM-DD", None)),
        None => None,
    };
    let until_ts = match args.until.as_deref().map(crate::dto::parse_iso_date) {
        Some(Some(ts)) => Some(ts + 86_400 - 1), // inclusive end-of-day
        Some(None) => return Err(McpError::invalid_params("until must be YYYY-MM-DD", None)),
        None => None,
    };

    let limit = args.limit.unwrap_or(LIMIT_DEFAULT).clamp(1, LIMIT_MAX);
    let k = std::cmp::max(limit * K_MULTIPLIER, K_MIN);

    // Force the USER_EMAIL scope. The caller's `account_ids` may only narrow
    // the owned set, never widen it. An empty scoped set (no owned account, or
    // the caller asked only for foreign ids) returns empty results — never an
    // unscoped "all accounts" query.
    let allowed = state
        .scope
        .allowed_account_ids(&state.storage)
        .await
        .map_err(internal_error)?;
    let scoped = scope::narrow(&allowed, &args.account_ids);
    if scoped.is_empty() {
        return Ok(SearchResponseDto {
            hits: Vec::new(),
            mode: mode.as_str().to_string(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
    }

    let search_resp = state
        .searcher
        .search(&SearchQuery {
            q: args.q.clone(),
            mode,
            k,
            account_ids: scoped.clone(),
        })
        .await
        .map_err(internal_error)?;

    let mut hits: Vec<SearchHitDto> = Vec::new();
    for hit in search_resp.hits.into_iter() {
        let row = match state.storage.get_message(hit.message_id.as_str()).await {
            Ok(Some(r)) if r.tombstoned_at.is_none() => r,
            Ok(_) => continue,
            Err(e) => return Err(internal_error(e)),
        };
        if !filter_match(&row, &args, &scoped, since_ts, until_ts) {
            continue;
        }
        let snippet = derive_snippet(&row.body_md, &hit.semantic_snippet, &args.q, mode);
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

    let _ = SNIPPET_CHARS; // documented snippet budget lives in derive_snippet
    Ok(SearchResponseDto {
        hits,
        mode: mode.as_str().to_string(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// `get_message` — full parsed message, or `not_found` if missing, tombstoned,
/// or owned by a foreign account.
pub async fn get_message(state: &McpState, args: IdArg) -> Result<MessageDto, McpError> {
    let allowed = state
        .scope
        .allowed_account_ids(&state.storage)
        .await
        .map_err(internal_error)?;
    match state.storage.get_message(&args.id).await {
        Ok(Some(row))
            if row.tombstoned_at.is_none() && scope::is_allowed(&allowed, &row.account_id) =>
        {
            Ok(message_row_to_dto(row))
        }
        Ok(_) => Err(not_found("no such message")),
        Err(e) => Err(internal_error(e)),
    }
}

/// `get_raw_message` — RFC 5322 source, same scope guard as `get_message`.
pub async fn get_raw_message(state: &McpState, args: IdArg) -> Result<RawMessageDto, McpError> {
    let allowed = state
        .scope
        .allowed_account_ids(&state.storage)
        .await
        .map_err(internal_error)?;
    let row = match state.storage.get_message(&args.id).await {
        Ok(Some(r)) if r.tombstoned_at.is_none() && scope::is_allowed(&allowed, &r.account_id) => r,
        Ok(_) => return Err(not_found("no such message")),
        Err(e) => return Err(internal_error(e)),
    };
    let bytes = tokio::fs::read(&row.raw_path).await.map_err(internal_error)?;
    Ok(RawMessageDto {
        message_id: row.message_id,
        raw: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

/// `get_thread` — every owned, non-tombstoned message in the thread, oldest
/// first; `not_found` if none remain after scope filtering.
pub async fn get_thread(state: &McpState, args: IdArg) -> Result<ThreadResponseDto, McpError> {
    let allowed = state
        .scope
        .allowed_account_ids(&state.storage)
        .await
        .map_err(internal_error)?;
    let rows = state
        .storage
        .get_thread(&args.id)
        .await
        .map_err(internal_error)?;
    let rows = retain_allowed(&allowed, rows, |r| r.account_id.as_str());
    if rows.is_empty() {
        return Err(not_found("no such thread"));
    }
    let messages: Vec<MessageDto> = rows.into_iter().map(message_row_to_dto).collect();
    Ok(ThreadResponseDto {
        thread_id: args.id,
        messages,
    })
}

/// `list_accounts` — the owned accounts only.
pub async fn list_accounts(state: &McpState) -> Result<AccountsResponseDto, McpError> {
    let allowed = state
        .scope
        .allowed_account_ids(&state.storage)
        .await
        .map_err(internal_error)?;
    let rows = state
        .storage
        .list_active_accounts()
        .await
        .map_err(internal_error)?;
    let accounts = retain_allowed(&allowed, rows, |a| a.account_id.as_str())
        .into_iter()
        .map(|a| AccountDto {
            account_id: a.account_id,
            folders: a.folders,
        })
        .collect();
    Ok(AccountsResponseDto { accounts })
}

/// `status` — daemon uptime, owned-account sync state, and the issue-#20
/// drainer/daemon crash-loop health. `ok = !in_crash_loop`.
pub async fn status(state: &McpState) -> Result<StatusResponseDto, McpError> {
    let allowed = state
        .scope
        .allowed_account_ids(&state.storage)
        .await
        .map_err(internal_error)?;
    let accounts = state
        .storage
        .list_active_accounts()
        .await
        .map_err(internal_error)?;
    let accounts = retain_allowed(&allowed, accounts, |a| a.account_id.as_str());

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
        out.push(AccountStatusDto {
            account_id: a.account_id,
            folders: a.folders,
            health: sync.as_ref().map(|s| format!("{:?}", s.account_health)),
            last_sync_unix: sync.as_ref().and_then(|s| s.last_full_sync_at),
            last_seen_uid: sync.as_ref().map(|s| s.last_seen_uid),
        });
    }

    let queue = state.storage.queue_health().await.map_err(internal_error)?;

    let daemon_health = state.daemon_health;
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let in_backoff = daemon_health
        .backoff_until_unix
        .map(|until| now_unix < until)
        .unwrap_or(false);

    let drainer = DrainerStatusDto {
        queue_depth: queue.depth,
        failed_permanent: queue.failed_permanent,
        last_index_error: queue.last_error.map(|e| LastIndexErrorDto {
            message_id: e.message_id,
            error: e.error,
            attempts: e.attempts,
        }),
    };
    let daemon = DaemonStatusDto {
        restart_count: daemon_health.restart_count,
        consecutive_crashes: daemon_health.consecutive_crashes,
        last_crash_unix: daemon_health.last_crash_unix,
        in_crash_loop: daemon_health.in_crash_loop,
        in_backoff,
        backoff_until_unix: daemon_health.backoff_until_unix,
    };

    Ok(StatusResponseDto {
        // A daemon in a crash loop is not healthy even though this request
        // happened to land in an up window (issue #20).
        ok: !daemon_health.in_crash_loop,
        uptime_secs: state.started_at.elapsed().as_secs(),
        accounts: out,
        drainer,
        daemon,
    })
}
