//! Integration tests for the witchcraft-backed indexer.
//!
//! The whole file is gated to Unix targets (where the witchcraft binding
//! resolves); it is an empty compilation unit elsewhere. The individual
//! tests are additionally `#[ignore]`-gated and only run when the
//! `XTR_ASSETS` env var points to a directory containing the pinned T5
//! GGUF weights file (downloaded via `scryd-fetch-weights`).
//!
//! Run with:
//!   XTR_ASSETS=/path/to/assets/ \
//!   cargo test -p scryd-search --test witchcraft -- --include-ignored
#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::path::PathBuf;

use scryd_search::indexer::Indexer;
use scryd_search::{IndexSubmit, MessageId, Mode, SearchQuery, WitchcraftIndexer};
use tempfile::TempDir;

fn assets_path() -> Option<PathBuf> {
    std::env::var_os("XTR_ASSETS").map(PathBuf::from)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires XTR_ASSETS env var pointing to T5 weights dir"]
async fn round_trip_submit_and_search() {
    let Some(assets) = assets_path() else {
        eprintln!("XTR_ASSETS not set; skipping");
        return;
    };
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("witchcraft.sqlite");

    let idx = WitchcraftIndexer::open(&db_path, &assets)
        .await
        .expect("open witchcraft");

    idx.submit(IndexSubmit {
        message_id: MessageId::new("msg-A"),
        document: "Subject: invoice\n\nThe annual invoice arrived today, please review.".into(),
    })
    .await
    .expect("submit A");

    idx.submit(IndexSubmit {
        message_id: MessageId::new("msg-B"),
        document: "Subject: birthday\n\nBirthday cake recipe inside.".into(),
    })
    .await
    .expect("submit B");

    let res = idx
        .search(&SearchQuery {
            q: "invoice".into(),
            mode: Mode::FullText,
            k: 5,
            account_ids: Vec::new(),
        })
        .await
        .expect("search");

    assert!(
        res.hits.iter().any(|h| h.message_id.as_str() == "msg-A"),
        "expected msg-A in fulltext results: {:?}",
        res.hits
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires XTR_ASSETS env var pointing to T5 weights dir"]
async fn truncate_clears_the_index() {
    let Some(assets) = assets_path() else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("witchcraft.sqlite");

    let idx = WitchcraftIndexer::open(&db_path, &assets).await.unwrap();
    idx.submit(IndexSubmit {
        message_id: MessageId::new("msg-x"),
        document: "doc body".into(),
    })
    .await
    .unwrap();
    idx.truncate().await.unwrap();

    let res = idx
        .search(&SearchQuery {
            q: "body".into(),
            mode: Mode::FullText,
            k: 5,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert!(res.hits.is_empty(), "truncate should drop all docs");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires XTR_ASSETS env var pointing to T5 weights dir"]
async fn remove_drops_a_single_message() {
    let Some(assets) = assets_path() else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("witchcraft.sqlite");

    let idx = WitchcraftIndexer::open(&db_path, &assets).await.unwrap();
    idx.submit(IndexSubmit {
        message_id: MessageId::new("msg-keep"),
        document: "keeper".into(),
    })
    .await
    .unwrap();
    idx.submit(IndexSubmit {
        message_id: MessageId::new("msg-drop"),
        document: "doomed".into(),
    })
    .await
    .unwrap();
    idx.remove(&MessageId::new("msg-drop")).await.unwrap();

    let res = idx
        .search(&SearchQuery {
            q: "doomed".into(),
            mode: Mode::FullText,
            k: 5,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert!(
        !res.hits.iter().any(|h| h.message_id.as_str() == "msg-drop"),
        "msg-drop should not appear after remove"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires XTR_ASSETS env var pointing to T5 weights dir"]
async fn multi_segment_doc_returns_single_hit_per_message() {
    // v0.3.7: emails larger than SEGMENT_TARGET_CHARS produce multiple
    // witchcraft documents under derived UUIDs. The search wrapper
    // dedups by message_id so callers still see one hit per email.
    let Some(assets) = assets_path() else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("witchcraft.sqlite");

    let idx = WitchcraftIndexer::open(&db_path, &assets).await.unwrap();

    // Build a long body that will split into ≥3 segments. Each segment
    // mentions "uniqueneedle" so semantic search has multiple per-doc
    // candidates and the dedup path is exercised.
    let chunk = "uniqueneedle ".repeat(50) + &"a".repeat(2000) + "\n\n";
    let big = chunk.repeat(4);
    idx.submit(IndexSubmit {
        message_id: MessageId::new("multi-seg"),
        document: big,
    })
    .await
    .expect("submit multi-segment doc");

    let res = idx
        .search(&SearchQuery {
            q: "uniqueneedle".into(),
            mode: Mode::FullText,
            k: 10,
            account_ids: Vec::new(),
        })
        .await
        .expect("search");

    let multi_seg_hits = res
        .hits
        .iter()
        .filter(|h| h.message_id.as_str() == "multi-seg")
        .count();
    assert_eq!(
        multi_seg_hits, 1,
        "dedup must collapse multi-segment results into one hit per message_id"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires XTR_ASSETS env var pointing to T5 weights dir"]
async fn remove_clears_all_segments_of_a_multi_segment_doc() {
    let Some(assets) = assets_path() else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("witchcraft.sqlite");

    let idx = WitchcraftIndexer::open(&db_path, &assets).await.unwrap();
    let big = "removeme ".repeat(50) + &"x".repeat(2000) + "\n\n";
    let big = big.repeat(4);
    idx.submit(IndexSubmit {
        message_id: MessageId::new("multi-drop"),
        document: big,
    })
    .await
    .unwrap();

    idx.remove(&MessageId::new("multi-drop")).await.unwrap();

    let res = idx
        .search(&SearchQuery {
            q: "removeme".into(),
            mode: Mode::FullText,
            k: 10,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert!(
        !res.hits.iter().any(|h| h.message_id.as_str() == "multi-drop"),
        "remove must drop every segment of a multi-segment doc"
    );
}
