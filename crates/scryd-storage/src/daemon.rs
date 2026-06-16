//! `daemon_runs` helpers. The runtime records one row per `scryd serve`
//! process so crashes are durable and inferable across restarts: a run whose
//! `stopped_at` is still NULL when the *next* process starts is a run that died
//! without a clean shutdown (panic/abort/SIGKILL/OOM). The startup path uses
//! the trailing-unclean streak to detect a crash loop and back off; `/status`
//! surfaces the same numbers so operators don't have to reconstruct them from
//! transient journal timing.

use rusqlite::params;

use crate::db::StorageError;
use crate::handle::StorageHandle;

/// Keep the most recent N run rows; older rows are pruned on each `begin_run`.
/// `run_id` is `AUTOINCREMENT`, so pruning never recycles ids — the max id
/// stays a monotonic lifetime restart counter even after old rows are dropped.
const KEEP_RUNS: i64 = 200;

/// Snapshot of crash history computed at process start, before the new run row
/// is observed by callers. All counts describe runs that preceded this one.
#[derive(Debug, Clone, Default)]
pub struct DaemonRunStats {
    /// Row id of the run just inserted. Monotonic across the daemon's
    /// lifetime; usable as a "this is the Nth start" restart counter.
    pub run_id: i64,
    /// Number of immediately-preceding runs that ended without a clean
    /// shutdown (counting back from the most recent until the first clean run).
    pub consecutive_unclean: u32,
    /// `started_at` of the most recent unclean run, if any.
    pub last_crash_unix: Option<i64>,
}

impl StorageHandle {
    /// Record the start of a daemon run and return the crash history that
    /// preceded it. Prunes the journal to the most recent [`KEEP_RUNS`] rows.
    pub async fn begin_run(
        &self,
        version: &str,
        pid: i64,
    ) -> Result<DaemonRunStats, StorageError> {
        let version = version.to_string();
        self.with_writer(move |conn| {
            // Count the trailing run of unclean shutdowns, newest first, and
            // remember when the most recent crash started. Stop at the first
            // cleanly-stopped run — that breaks the streak.
            let mut consecutive_unclean: u32 = 0;
            let mut last_crash_unix: Option<i64> = None;
            {
                let mut stmt = conn.prepare(
                    "SELECT started_at, stopped_at FROM daemon_runs ORDER BY run_id DESC",
                )?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    let started_at: i64 = row.get(0)?;
                    let stopped_at: Option<i64> = row.get(1)?;
                    if stopped_at.is_some() {
                        break;
                    }
                    consecutive_unclean = consecutive_unclean.saturating_add(1);
                    if last_crash_unix.is_none() {
                        last_crash_unix = Some(started_at);
                    }
                }
            }

            conn.execute(
                "INSERT INTO daemon_runs (started_at, stopped_at, clean, version, pid) \
                 VALUES (strftime('%s','now'), NULL, 0, ?1, ?2)",
                params![version, pid],
            )?;
            let run_id = conn.last_insert_rowid();

            // Bound the journal. Pruning by id keeps the AUTOINCREMENT high-water
            // mark intact, so `run_id` remains a lifetime restart counter.
            conn.execute(
                "DELETE FROM daemon_runs WHERE run_id <= ?1 - ?2",
                params![run_id, KEEP_RUNS],
            )?;

            Ok(DaemonRunStats {
                run_id,
                consecutive_unclean,
                last_crash_unix,
            })
        })
        .await
    }

    /// Mark a run as cleanly stopped. Called from the graceful-shutdown path so
    /// the next process does not count this run as a crash.
    pub async fn finish_run(&self, run_id: i64) -> Result<(), StorageError> {
        self.with_writer(move |conn| {
            conn.execute(
                "UPDATE daemon_runs SET stopped_at = strftime('%s','now'), clean = 1 \
                 WHERE run_id = ?1",
                params![run_id],
            )?;
            Ok(())
        })
        .await
    }
}
