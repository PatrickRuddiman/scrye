use scryd_storage::open;
use tempfile::TempDir;

#[test]
fn fresh_db_uses_wal_journal_mode() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("meta.sqlite");
    let conn = open(&path).expect("open");

    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn synchronous_pragma_is_normal() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("meta.sqlite");
    let conn = open(&path).expect("open");

    // synchronous=NORMAL is reported as the integer 1.
    let value: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    assert_eq!(value, 1);
}

#[test]
fn foreign_keys_pragma_on() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("meta.sqlite");
    let conn = open(&path).expect("open");

    let value: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(value, 1);
}
