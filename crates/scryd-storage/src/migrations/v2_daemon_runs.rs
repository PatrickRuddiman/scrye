//! Migration v2: daemon-run journal + per-item failure timestamp.
//!
//! `daemon_runs` lets the daemon infer crashes across restarts (a row whose
//! `stopped_at` is still NULL is a run that died without a clean shutdown), so
//! a crash loop is durable and observable rather than visible only in the
//! journal's transient timing. `index_queue.last_failed_at` records when a
//! row last failed so `queue_health` can surface the *most recent* index error.

pub const SQL: &str = r#"
CREATE TABLE daemon_runs (
    run_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at  INTEGER NOT NULL,
    stopped_at  INTEGER NULL,
    clean       INTEGER NOT NULL DEFAULT 0,
    version     TEXT    NULL,
    pid         INTEGER NULL
);

CREATE INDEX idx_daemon_runs_started ON daemon_runs (started_at DESC);

ALTER TABLE index_queue ADD COLUMN last_failed_at INTEGER NULL;
"#;
