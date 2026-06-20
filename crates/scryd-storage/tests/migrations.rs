use rusqlite::Connection;
use scryd_storage::{migrations, open};
use tempfile::TempDir;

fn fresh_db() -> (TempDir, std::path::PathBuf, Connection) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("meta.sqlite");
    let conn = open(&path).expect("open");
    (dir, path, conn)
}

fn collect_table_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn collect_index_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master \
             WHERE type='index' AND name NOT LIKE 'sqlite_autoindex_%' \
             ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn fresh_db_records_latest_migration_version() {
    let (_dir, _, conn) = fresh_db();
    let v: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, migrations::latest_version() as i64);
}

#[test]
fn all_expected_tables_exist() {
    let (_dir, _, conn) = fresh_db();
    let tables = collect_table_names(&conn);
    for required in [
        "accounts",
        "attachments",
        "daemon_runs",
        "index_queue",
        "messages",
        "schema_version",
        "sync_state",
    ] {
        assert!(
            tables.contains(&required.to_string()),
            "table {required} missing; have {tables:?}"
        );
    }
}

#[test]
fn all_expected_indexes_exist() {
    let (_dir, _, conn) = fresh_db();
    let indexes = collect_index_names(&conn);
    for required in [
        "idx_attachments_message",
        "idx_daemon_runs_started",
        "idx_index_queue_drainer",
        "idx_messages_account",
        "idx_messages_date",
        "idx_messages_folder",
        "idx_messages_sender",
        "idx_messages_thread",
    ] {
        assert!(
            indexes.contains(&required.to_string()),
            "index {required} missing; have {indexes:?}"
        );
    }
    assert!(
        indexes.len() >= 8,
        "expected at least 8 indexes, got {} ({indexes:?})",
        indexes.len()
    );
}

#[test]
fn unique_constraint_present_on_messages() {    let (_dir, _, conn) = fresh_db();
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='messages'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        sql.contains("UNIQUE (account_id, folder, server_uid, uidvalidity)"),
        "unique constraint missing in messages DDL:\n{sql}"
    );
}

#[test]
fn reopen_does_not_reapply_migrations() {
    let (_dir, path, _conn) = fresh_db();
    // Drop the first connection by shadowing; re-open the same DB.
    let conn2 = open(&path).expect("re-open");
    let count: i64 = conn2
        .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        count,
        migrations::latest_version() as i64,
        "schema_version row count grew on re-open"
    );
}

#[test]
fn foreign_keys_enforced() {
    let (_dir, _, conn) = fresh_db();
    // accounts must exist before sync_state can refer to it.
    let result = conn.execute(
        "INSERT INTO sync_state (account_id, folder) VALUES ('ghost', 'INBOX')",
        [],
    );
    assert!(
        result.is_err(),
        "FK enforcement absent: ghost account_id was accepted"
    );
}

#[test]
fn index_queue_has_last_failed_at_column() {
    let (_dir, _, conn) = fresh_db();
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='index_queue'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        sql.contains("last_failed_at"),
        "v2 column missing in index_queue DDL:\n{sql}"
    );
}

#[test]
fn daemon_runs_run_id_autoincrements() {
    let (_dir, _, conn) = fresh_db();
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='daemon_runs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        sql.contains("AUTOINCREMENT"),
        "daemon_runs.run_id must AUTOINCREMENT so ids stay monotonic across pruning:\n{sql}"
    );
}
