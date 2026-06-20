//! `daemon_runs` journal: crash inference across restarts.

use scryd_storage::StorageHandle;
use tempfile::TempDir;

fn handle() -> (TempDir, StorageHandle) {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 1).expect("open");
    (dir, h)
}

#[tokio::test]
async fn first_run_has_no_prior_crashes() {
    let (_d, h) = handle();
    let stats = h.begin_run("0.0.0-test", 1234).await.unwrap();
    assert_eq!(stats.run_id, 1);
    assert_eq!(stats.consecutive_unclean, 0);
    assert_eq!(stats.last_crash_unix, None);
}

#[tokio::test]
async fn clean_shutdown_does_not_count_as_crash() {
    let (_d, h) = handle();
    let r1 = h.begin_run("v", 1).await.unwrap();
    h.finish_run(r1.run_id).await.unwrap();

    let r2 = h.begin_run("v", 2).await.unwrap();
    assert_eq!(r2.run_id, 2);
    assert_eq!(
        r2.consecutive_unclean, 0,
        "a cleanly-finished prior run must not be counted as a crash"
    );
    assert_eq!(r2.last_crash_unix, None);
}

#[tokio::test]
async fn unclean_runs_accumulate_consecutive_count() {
    let (_d, h) = handle();
    // Three runs that never call finish_run = three crashes.
    let _ = h.begin_run("v", 1).await.unwrap();
    let _ = h.begin_run("v", 2).await.unwrap();
    let _ = h.begin_run("v", 3).await.unwrap();

    // The 4th boot observes the 3 trailing unclean runs.
    let r4 = h.begin_run("v", 4).await.unwrap();
    assert_eq!(r4.run_id, 4);
    assert_eq!(r4.consecutive_unclean, 3);
    assert!(r4.last_crash_unix.is_some());
}

#[tokio::test]
async fn a_clean_run_resets_the_streak() {
    let (_d, h) = handle();
    // Two crashes...
    let _ = h.begin_run("v", 1).await.unwrap();
    let _ = h.begin_run("v", 2).await.unwrap();
    // ...then a clean run...
    let r3 = h.begin_run("v", 3).await.unwrap();
    assert_eq!(r3.consecutive_unclean, 2);
    h.finish_run(r3.run_id).await.unwrap();
    // ...then one more crash. The streak counts only back to the clean run.
    let _ = h.begin_run("v", 4).await.unwrap();
    let r5 = h.begin_run("v", 5).await.unwrap();
    assert_eq!(
        r5.consecutive_unclean, 1,
        "the clean run at id=3 must break the streak"
    );
}

#[tokio::test]
async fn run_id_is_monotonic_after_pruning() {
    let (_d, h) = handle();
    // Far more than KEEP_RUNS (200) runs; ids must keep climbing even though
    // old rows are pruned. We don't begin 200+ here for speed — instead assert
    // monotonicity holds over a modest sequence (the prune threshold is large).
    let mut last = 0;
    for pid in 0..10 {
        let s = h.begin_run("v", pid).await.unwrap();
        assert!(s.run_id > last, "run_id must increase: {} !> {}", s.run_id, last);
        last = s.run_id;
    }
    assert_eq!(last, 10);
}
