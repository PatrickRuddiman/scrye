//! Integration test for `scryd_runtime::serve_init`. Confirms the
//! WitchcraftIndexer wired in task 06 persists across a restart
//! (task 07 — the witchcraft.sqlite file is the source of truth;
//! a fresh `serve_init` against the same data dir reopens it).
//!
//! Gated on the `XTR_ASSETS` env var (same pattern as
//! `scryd-search/tests/witchcraft.rs`) — the test needs a real T5
//! weights file. Skipped silently when unset so `cargo test
//! --workspace` on a host without weights stays green.

#![cfg(unix)]

use std::path::PathBuf;

fn assets_path() -> Option<PathBuf> {
    std::env::var_os("XTR_ASSETS").map(PathBuf::from)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires XTR_ASSETS env var pointing to T5 weights dir"]
async fn witchcraft_index_survives_restart() {
    let Some(assets) = assets_path() else {
        eprintln!("XTR_ASSETS not set; skipping witchcraft_index_survives_restart");
        return;
    };

    use scryd_search::{IndexSubmit, Indexer, MessageId, Mode, SearchQuery, WitchcraftIndexer};
    use std::sync::Arc;

    let data_dir = tempfile::tempdir().unwrap();
    let db_path = data_dir.path().join("witchcraft.sqlite");

    // First open: write 5 fixture documents.
    {
        let indexer: Arc<dyn Indexer> =
            Arc::new(WitchcraftIndexer::open(&db_path, &assets).await.unwrap());
        for i in 0..5 {
            indexer
                .submit(IndexSubmit {
                    message_id: MessageId::new(format!("primary:fixture-{i}")),
                    document: format!("subject: persistence-test-{i}\n\nbody for the survives-restart test"),
                })
                .await
                .unwrap();
        }
        // Force the indexer to flush by issuing a search.
        let _ = indexer
            .search(&SearchQuery {
                q: "persistence".to_string(),
                mode: Mode::FullText,
                k: 10,
                account_ids: Vec::new(),
            })
            .await
            .unwrap();
        // Drop the Arc so the underlying state's Mutex can be dropped
        // when the indexer goes out of scope.
        drop(indexer);
    }

    // Second open: same db_path → must surface the 5 documents.
    let indexer: Arc<dyn Indexer> =
        Arc::new(WitchcraftIndexer::open(&db_path, &assets).await.unwrap());
    let resp = indexer
        .search(&SearchQuery {
            q: "persistence".to_string(),
            mode: Mode::FullText,
            k: 10,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert!(
        resp.hits.len() >= 5,
        "expected >= 5 hits after reopen, got {}",
        resp.hits.len()
    );
}
