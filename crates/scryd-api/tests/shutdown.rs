#![cfg(unix)]

use std::time::Duration;

use scryd_api::{bind, router, serve, AppState};
use scryd_search::InMemoryIndexer;
use scryd_storage::StorageHandle;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::oneshot;

#[tokio::test]
async fn serve_returns_when_shutdown_signal_resolves() {
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");
    let listener = bind(&scryd_dir).await.unwrap();

    let storage_dir = dir.path().join("data");
    let storage = StorageHandle::open(&storage_dir, 1).unwrap();
    let indexer: Arc<dyn scryd_search::Indexer> = Arc::new(InMemoryIndexer::new());
    let state = AppState::new(storage, indexer, scryd_config::Config::default());
    let r = router(state);

    let (tx, rx) = oneshot::channel::<()>();
    let serve_task = tokio::spawn(async move {
        serve(listener, r, async {
            let _ = rx.await;
        })
        .await
    });

    // Give serve a moment to enter its accept loop.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = tx.send(());

    // serve must return promptly after shutdown signal.
    let result = tokio::time::timeout(Duration::from_secs(2), serve_task)
        .await
        .expect("serve returns after shutdown")
        .expect("task joined");
    assert!(result.is_ok(), "serve returned: {result:?}");
}
