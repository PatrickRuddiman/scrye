#![cfg(unix)]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use scryd_api::{router, AppState};
use scryd_search::{InMemoryIndexer, Indexer, IndexSubmit, MessageId};
use scryd_storage::{Address, MessageInsert, StorageHandle};
use tempfile::TempDir;
use tower::ServiceExt; // for `oneshot`

const FIXTURE: &[u8] = b"From: Alice <alice@example.com>\r\n\
                         To: Bob <bob@example.com>\r\n\
                         Subject: invoice\r\n\
                         Date: Thu, 30 Apr 2026 12:00:00 +0000\r\n\
                         Message-ID: <fixture@example.com>\r\n\
                         Content-Type: text/plain; charset=utf-8\r\n\r\n\
                         The April invoice is attached. Please review at your\
                         earliest convenience and acknowledge receipt.\r\n";

async fn setup() -> (TempDir, AppState) {
    let dir = TempDir::new().unwrap();
    let storage = StorageHandle::open(dir.path(), 2).unwrap();
    storage
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

    // Seed a message via storage + write the raw .eml.
    let raw_path = scryd_storage::raw::write_raw(
        dir.path(),
        "primary",
        "primary:fixture@example.com",
        1_777_000_000,
        FIXTURE,
    )
    .await
    .unwrap();
    let insert = MessageInsert {
        message_id: "primary:fixture@example.com".to_string(),
        account_id: "primary".to_string(),
        folder: "INBOX".to_string(),
        server_uid: 1,
        uidvalidity: 1,
        header_message_id: Some("fixture@example.com".to_string()),
        in_reply_to: None,
        references: vec![],
        sender_addr: "alice@example.com".to_string(),
        sender_name: Some("Alice".to_string()),
        recipients_to: vec![Address {
            addr: "bob@example.com".to_string(),
            name: None,
        }],
        recipients_cc: vec![],
        subject: Some("invoice".to_string()),
        date_unix: 1_777_000_000,
        raw_path: raw_path.to_string_lossy().into_owned(),
        body_md: "The April invoice is attached. Please review at your earliest convenience.".to_string(),
        size_bytes: FIXTURE.len() as u64,
    };
    storage.insert_message(insert).await.unwrap();

    // Index into the in-memory backend.
    let indexer = Arc::new(InMemoryIndexer::new());
    indexer
        .submit(IndexSubmit {
            message_id: MessageId::new("primary:fixture@example.com"),
            document: "invoice\n\nAlice <alice@example.com>\n\nThe April invoice is attached".to_string(),
        })
        .await
        .unwrap();

    let state = AppState::new(storage, indexer);
    (dir, state)
}

async fn request(state: AppState, uri: &str) -> (StatusCode, axum::body::Bytes, Vec<(String, String)>) {
    let app = router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    (status, body, headers)
}

fn parse_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("valid JSON response")
}

#[tokio::test]
async fn search_returns_indexed_message() {
    let (_d, state) = setup().await;
    let (status, body, _) = request(state, "/search?q=invoice").await;
    assert_eq!(status, StatusCode::OK);
    let v = parse_json(&body);
    assert_eq!(v["mode"], "fulltext");
    let hits = v["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["message_id"], "primary:fixture@example.com");
    assert_eq!(hits[0]["sender_addr"], "alice@example.com");
    assert!(hits[0]["snippet"].as_str().unwrap().contains("invoice"));
}

#[tokio::test]
async fn search_with_invalid_mode_returns_bad_query() {
    let (_d, state) = setup().await;
    let (status, body, _) = request(state, "/search?mode=garbage").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let v = parse_json(&body);
    assert_eq!(v["error"]["code"], "bad_query");
}

#[tokio::test]
async fn search_with_invalid_date_returns_bad_query() {
    let (_d, state) = setup().await;
    let (status, body, _) = request(state, "/search?since=not-a-date").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let v = parse_json(&body);
    assert_eq!(v["error"]["code"], "bad_query");
}

#[tokio::test]
async fn search_filter_by_account_excludes_non_matching() {
    let (_d, state) = setup().await;
    let (status, body, _) = request(state, "/search?q=invoice&account=other").await;
    assert_eq!(status, StatusCode::OK);
    let v = parse_json(&body);
    assert_eq!(v["hits"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn message_returns_full_shape() {
    let (_d, state) = setup().await;
    let (status, body, _) =
        request(state, "/message/primary:fixture@example.com").await;
    assert_eq!(status, StatusCode::OK);
    let v = parse_json(&body);
    assert_eq!(v["message_id"], "primary:fixture@example.com");
    assert_eq!(v["from"]["addr"], "alice@example.com");
    assert_eq!(v["from"]["name"], "Alice");
    assert_eq!(v["to"][0]["addr"], "bob@example.com");
    assert_eq!(v["subject"], "invoice");
    assert!(v["body_md"].as_str().unwrap().contains("invoice"));
}

#[tokio::test]
async fn message_unknown_returns_404_not_found() {
    let (_d, state) = setup().await;
    let (status, body, _) = request(state, "/message/no-such@id").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let v = parse_json(&body);
    assert_eq!(v["error"]["code"], "not_found");
}

#[tokio::test]
async fn message_tombstoned_returns_404() {
    let (_d, state) = setup().await;
    state
        .storage
        .tombstone("primary:fixture@example.com")
        .await
        .unwrap();
    let (status, _body, _) =
        request(state, "/message/primary:fixture@example.com").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn raw_streams_eml_with_message_rfc822_content_type() {
    let (_d, state) = setup().await;
    let (status, body, headers) =
        request(state, "/message/primary:fixture@example.com/raw").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v == "message/rfc822"));
    assert!(body.starts_with(b"From: Alice"));
}

#[tokio::test]
async fn thread_returns_messages_oldest_first() {
    let (_d, state) = setup().await;
    let row = state
        .storage
        .get_message("primary:fixture@example.com")
        .await
        .unwrap()
        .unwrap();
    let (status, body, _) = request(state, &format!("/thread/{}", row.thread_id)).await;
    assert_eq!(status, StatusCode::OK);
    let v = parse_json(&body);
    assert_eq!(v["thread_id"], row.thread_id);
    assert_eq!(v["messages"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn thread_unknown_returns_404() {
    let (_d, state) = setup().await;
    let (status, _, _) = request(state, "/thread/nonexistent-thread").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn accounts_lists_active_accounts_with_folders() {
    let (_d, state) = setup().await;
    let (status, body, _) = request(state, "/accounts").await;
    assert_eq!(status, StatusCode::OK);
    let v = parse_json(&body);
    let accounts = v["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0]["account_id"], "primary");
    let folders = accounts[0]["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0], "INBOX");
}

#[tokio::test]
async fn search_limit_is_clamped_to_200() {
    let (_d, state) = setup().await;
    // Request 999, must not return error — limit is silently clamped.
    let (status, _, _) = request(state, "/search?q=invoice&limit=999").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn search_with_empty_query_returns_results_too() {
    let (_d, state) = setup().await;
    let (status, body, _) = request(state, "/search").await;
    assert_eq!(status, StatusCode::OK);
    let v = parse_json(&body);
    // The InMemoryIndexer returns all docs for an empty query.
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
}
