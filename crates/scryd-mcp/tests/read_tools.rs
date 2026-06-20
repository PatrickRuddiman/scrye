//! Read-tool scope enforcement: every read tool returns only owned-account
//! data, and any foreign id is indistinguishable from a missing one
//! (`resource_not_found`, no existence leak).

mod common;

use common::*;
use rmcp::model::ErrorCode;
use scryd_mcp::dto::{IdArg, SearchArgs};
use scryd_mcp::tools::read;

fn search_args(q: &str) -> SearchArgs {
    SearchArgs {
        q: q.to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn search_returns_only_owned_hits() {
    let seed = seed().await;
    let state = seed.state();
    // Both messages' documents contain COMMON_TERM, so the in-memory index
    // returns both; only the scope post-filter keeps the result owned-only.
    let resp = read::search(&state, search_args(COMMON_TERM)).await.unwrap();
    let ids: Vec<&str> = resp.hits.iter().map(|h| h.message_id.as_str()).collect();
    assert_eq!(ids, vec![OWNED_MSG]);
    assert!(resp.hits.iter().all(|h| h.account_id == OWNED_ID));
}

#[tokio::test]
async fn search_with_foreign_account_filter_is_empty() {
    let seed = seed().await;
    let state = seed.state();
    let args = SearchArgs {
        q: COMMON_TERM.to_string(),
        account_ids: vec![FOREIGN_ID.to_string()],
        ..Default::default()
    };
    // Narrowing the owned set by a foreign id yields an empty scoped set, which
    // must short-circuit to empty results — never an unscoped "all" query.
    let resp = read::search(&state, args).await.unwrap();
    assert!(resp.hits.is_empty());
}

#[tokio::test]
async fn get_message_owned_ok_foreign_not_found() {
    let seed = seed().await;
    let state = seed.state();
    let owned = read::get_message(&state, IdArg { id: OWNED_MSG.to_string() })
        .await
        .unwrap();
    assert_eq!(owned.account_id, OWNED_ID);
    assert_eq!(owned.message_id, OWNED_MSG);

    let err = read::get_message(&state, IdArg { id: FOREIGN_MSG.to_string() })
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
}

#[tokio::test]
async fn foreign_and_missing_messages_are_indistinguishable() {
    let seed = seed().await;
    let state = seed.state();
    let foreign = read::get_message(&state, IdArg { id: FOREIGN_MSG.to_string() })
        .await
        .unwrap_err();
    let missing = read::get_message(&state, IdArg { id: "no-such-id".to_string() })
        .await
        .unwrap_err();
    // Same code AND same message — a foreign id must not leak that it exists.
    assert_eq!(foreign.code, missing.code);
    assert_eq!(foreign.message, missing.message);
}

#[tokio::test]
async fn get_raw_message_owned_ok_foreign_not_found() {
    let seed = seed().await;
    let state = seed.state();
    let raw = read::get_raw_message(&state, IdArg { id: OWNED_MSG.to_string() })
        .await
        .unwrap();
    assert_eq!(raw.message_id, OWNED_MSG);
    assert!(raw.raw.contains("quarterly report"));

    let err = read::get_raw_message(&state, IdArg { id: FOREIGN_MSG.to_string() })
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
}

#[tokio::test]
async fn get_thread_owned_ok_foreign_not_found() {
    let seed = seed().await;
    let state = seed.state();
    let thread = read::get_thread(&state, IdArg { id: seed.owned_thread.clone() })
        .await
        .unwrap();
    assert!(!thread.messages.is_empty());
    assert!(thread.messages.iter().all(|m| m.account_id == OWNED_ID));

    let err = read::get_thread(&state, IdArg { id: seed.foreign_thread.clone() })
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
}

#[tokio::test]
async fn list_accounts_returns_only_owned() {
    let seed = seed().await;
    let state = seed.state();
    let resp = read::list_accounts(&state).await.unwrap();
    let ids: Vec<&str> = resp.accounts.iter().map(|a| a.account_id.as_str()).collect();
    assert_eq!(ids, vec![OWNED_ID]);
}

#[tokio::test]
async fn status_scopes_accounts_and_reports_ok() {
    let seed = seed().await;
    let state = seed.state();
    let status = read::status(&state).await.unwrap();
    // Default daemon health is healthy → ok = !in_crash_loop = true.
    assert!(status.ok);
    let ids: Vec<&str> = status.accounts.iter().map(|a| a.account_id.as_str()).collect();
    assert_eq!(ids, vec![OWNED_ID]);
}

#[tokio::test]
async fn search_cannot_widen_via_foreign_account_id() {
    let seed = seed().await;
    let state = seed.state();
    // Caller explicitly asks for owned + foreign; narrowing keeps only owned,
    // so the foreign hit is dropped even though it was named.
    let args = SearchArgs {
        q: COMMON_TERM.to_string(),
        account_ids: vec![OWNED_ID.to_string(), FOREIGN_ID.to_string()],
        ..Default::default()
    };
    let resp = read::search(&state, args).await.unwrap();
    let ids: Vec<&str> = resp.hits.iter().map(|h| h.message_id.as_str()).collect();
    assert_eq!(ids, vec![OWNED_MSG]);
}

#[tokio::test]
async fn empty_scope_returns_nothing_never_all() {
    let seed = seed().await;
    // A USER_EMAIL that owns no account must never fall back to "all".
    let state = seed.state_with_scope("nobody@example.com");

    let search = read::search(&state, search_args(COMMON_TERM)).await.unwrap();
    assert!(search.hits.is_empty());

    let accounts = read::list_accounts(&state).await.unwrap();
    assert!(accounts.accounts.is_empty());

    let status = read::status(&state).await.unwrap();
    assert!(status.accounts.is_empty());

    // Even a real, existing message id is not_found when scope owns nothing.
    let err = read::get_message(&state, IdArg { id: OWNED_MSG.to_string() })
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
}
