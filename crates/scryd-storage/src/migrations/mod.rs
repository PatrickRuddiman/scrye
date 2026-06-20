//! Forward-only migration runner. Each registered migration is `(version,
//! sql)`; the runner applies any whose version is greater than the highest
//! one currently recorded in `schema_version`, inside a transaction, and then
//! records the version. There are no down migrations; recovery from a botched
//! migration is a follow-up forward migration.

mod v1_initial;
mod v2_daemon_runs;

use rusqlite::{Connection, Transaction};

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("read schema_version: {0}")]
    ReadVersion(#[source] rusqlite::Error),
    #[error("apply migration v{0}: {1}")]
    Apply(u32, #[source] rusqlite::Error),
    #[error("transaction: {0}")]
    Transaction(#[source] rusqlite::Error),
    #[error("create schema_version: {0}")]
    Bootstrap(#[source] rusqlite::Error),
}

/// Ordered registry of every migration this binary knows. Append-only.
const MIGRATIONS: &[(u32, &str)] = &[(1, v1_initial::SQL), (2, v2_daemon_runs::SQL)];

/// Apply any pending migrations. Safe to re-run on a fully migrated DB
/// (no migrations execute, no rows added).
pub fn run(conn: &mut Connection) -> Result<(), MigrationError> {
    bootstrap_schema_version(conn)?;
    let current = current_version(conn)?;

    for (version, sql) in MIGRATIONS {
        if (*version as i64) <= current {
            continue;
        }
        let tx = conn
            .transaction()
            .map_err(MigrationError::Transaction)?;
        apply_one(&tx, *version, sql)?;
        tx.commit().map_err(MigrationError::Transaction)?;
    }

    Ok(())
}

fn bootstrap_schema_version(conn: &Connection) -> Result<(), MigrationError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
             version INTEGER PRIMARY KEY,
             applied_at INTEGER NOT NULL
         );",
    )
    .map_err(MigrationError::Bootstrap)
}

fn current_version(conn: &Connection) -> Result<i64, MigrationError> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map_err(MigrationError::ReadVersion)
}

fn apply_one(tx: &Transaction<'_>, version: u32, sql: &str) -> Result<(), MigrationError> {
    tx.execute_batch(sql)
        .map_err(|e| MigrationError::Apply(version, e))?;
    tx.execute(
        "INSERT INTO schema_version (version, applied_at) VALUES (?1, strftime('%s','now'))",
        rusqlite::params![version],
    )
    .map_err(|e| MigrationError::Apply(version, e))?;
    Ok(())
}

/// Highest registered migration version. Tests assert
/// `schema_version.MAX(version) == latest_version()` after `run()`.
pub fn latest_version() -> u32 {
    MIGRATIONS.iter().map(|(v, _)| *v).max().unwrap_or(0)
}
