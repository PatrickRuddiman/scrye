#![cfg(unix)]

use std::io::Write as _;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use scryd_api::{router, AppState};
use scryd_search::{InMemoryIndexer, Indexer, IndexSubmit, MessageId};
use scryd_storage::StorageHandle;
use tempfile::{NamedTempFile, TempDir};
use tower::ServiceExt;

async fn fresh_state(config: scryd_config::Config) -> (TempDir, AppState, Arc<InMemoryIndexer>) {
    let dir = TempDir::new().unwrap();
    let storage = StorageHandle::open(dir.path(), 2).unwrap();
    let indexer = Arc::new(InMemoryIndexer::new());
    let state = AppState::new(storage, indexer.clone() as Arc<dyn Indexer>, config);
    (dir, state, indexer)
}

fn cfg_from_toml(toml: &str) -> scryd_config::Config {
    let mut tf = NamedTempFile::new().unwrap();
    tf.write_all(toml.as_bytes()).unwrap();
    scryd_config::Config::load(tf.path()).unwrap()
}

async fn post(state: AppState, uri: &str) -> (StatusCode, axum::body::Bytes) {
    let app = router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    (status, body)
}

fn parse_json(b: &[u8]) -> serde_json::Value {
    serde_json::from_slice(b).expect("valid JSON")
}

#[tokio::test]
async fn sync_returns_202_with_active_account_scope() {
    let cfg = cfg_from_toml(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"
"#,
    );
    let (_d, state, _idx) = fresh_state(cfg).await;
    state.storage.reconcile_from_config(&*state.config.read().await).await.unwrap();

    let (status, body) = post(state, "/sync").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let v = parse_json(&body);
    assert_eq!(v["started"], true);
    let accounts = v["scope"]["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0], "primary");
}

#[tokio::test]
async fn reindex_returns_202_truncates_indexer_and_repopulates_queue() {
    let (_d, state, indexer) = fresh_state(scryd_config::Config::default()).await;

    // Pre-load: seed an account, a message row, and an indexed document.
    state
        .storage
        .with_writer(|conn| {
            conn.execute(
                "INSERT INTO accounts (account_id, host, port, username, folders_json, active, mirrored_at) \
                 VALUES ('primary', 'imap.example.com', 993, 'u', '[\"INBOX\"]', 1, 0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    seed_message(&state.storage, "primary:m1@x", 1).await;
    indexer
        .submit(IndexSubmit {
            message_id: MessageId::new("primary:m1@x"),
            document: "indexed".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(indexer.len(), 1);

    let (status, body) = post(state.clone(), "/internal/reindex").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let v = parse_json(&body);
    assert_eq!(v["started"], true);

    // Indexer was truncated; queue has one row.
    assert_eq!(indexer.len(), 0);
    let queue = state.storage.list_queue().await.unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].message_id, "primary:m1@x");
}

#[tokio::test]
async fn reindex_concurrent_returns_409_conflict() {
    let (_d, state, _idx) = fresh_state(scryd_config::Config::default()).await;

    // First reindex acquires the lock and stays held.
    let (s1, _b1) = post(state.clone(), "/internal/reindex").await;
    assert_eq!(s1, StatusCode::ACCEPTED);

    // Second concurrent request hits the held mutex → 409.
    let (s2, body) = post(state, "/internal/reindex").await;
    assert_eq!(s2, StatusCode::CONFLICT);
    let v = parse_json(&body);
    assert_eq!(v["error"]["code"], "conflict");
}

#[tokio::test]
async fn reconcile_returns_diff_for_added_accounts() {
    let cfg = cfg_from_toml(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"
"#,
    );
    let (_d, state, _idx) = fresh_state(cfg).await;

    let (status, body) = post(state, "/internal/reconcile").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let v = parse_json(&body);
    assert_eq!(v["started"], true);
    let added = v["accounts_added"].as_array().unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0], "primary");
}

#[tokio::test]
async fn reconcile_inactivates_removed_accounts_on_second_call() {
    let two_accounts = cfg_from_toml(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"

[[accounts]]
id = "work"
host = "imap.work.com"
port = 993
user = "alice@work.com"
password = "q"
"#,
    );
    let (_d, state, _idx) = fresh_state(two_accounts).await;
    let _ = post(state.clone(), "/internal/reconcile").await;

    // Hot-swap config to one account; reconcile should mark "work" inactive.
    let one_account = cfg_from_toml(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"
"#,
    );
    *state.config.write().await = one_account;

    let (status, body) = post(state, "/internal/reconcile").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let v = parse_json(&body);
    let inact = v["accounts_inactivated"].as_array().unwrap();
    assert_eq!(inact.len(), 1);
    assert_eq!(inact[0], "work");
}

async fn seed_message(storage: &StorageHandle, id: &str, uid: u32) {
    use scryd_storage::{Address, MessageInsert};
    storage
        .insert_message(MessageInsert {
            message_id: id.to_string(),
            account_id: "primary".to_string(),
            folder: "INBOX".to_string(),
            server_uid: uid,
            uidvalidity: 1,
            header_message_id: Some(format!("{id}-h")),
            in_reply_to: None,
            references: vec![],
            sender_addr: "alice@example.com".to_string(),
            sender_name: None,
            recipients_to: vec![Address {
                addr: "bob@example.com".to_string(),
                name: None,
            }],
            recipients_cc: vec![],
            subject: Some("subj".to_string()),
            date_unix: 1_700_000_000,
            raw_path: "/tmp/x.eml".to_string(),
            body_md: "body".to_string(),
            size_bytes: 4,
        })
        .await
        .unwrap();
}

/// Verify that `POST /internal/reconcile` reloads config from disk when
/// `AppState::config_path` is set. Without the fix for issue #9, the
/// handler would read the stale in-memory config and miss the new account.
#[tokio::test]
async fn reconcile_picks_up_account_added_on_disk() {
    // Start with an empty in-memory config (no accounts).
    let (_d, mut state, _idx) = fresh_state(scryd_config::Config::default()).await;

    // Write a config with one account to a temp file.  NamedTempFile
    // creates with 0o600 mode (no world bits) so Config::load's
    // permission check passes on Unix.
    let mut cfg_file = NamedTempFile::new().unwrap();
    cfg_file
        .write_all(
            br#"
[[accounts]]
id = "live"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "secret"
"#,
        )
        .unwrap();

    // Point the state at the on-disk config.
    state.config_path = Some(cfg_file.path().to_path_buf());

    let (status, body) = post(state, "/internal/reconcile").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let v = parse_json(&body);
    assert_eq!(v["started"], true);
    // The handler must have reloaded from disk — "live" would be absent
    // if it had read the empty startup config instead.
    let added = v["accounts_added"].as_array().unwrap();
    assert_eq!(added.len(), 1, "expected exactly one account added");
    assert_eq!(added[0], "live");
}
