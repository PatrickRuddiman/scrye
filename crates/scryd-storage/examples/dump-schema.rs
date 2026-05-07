//! Open a temp meta.sqlite, run migrations, and print every table's CREATE
//! statement to stdout. Used by the task 03 acceptance check.

use rusqlite::Connection;
use scryd_storage::open;

fn main() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("meta.sqlite");
    let conn: Connection = open(&path).expect("open");

    let mut stmt = conn
        .prepare(
            "SELECT sql FROM sqlite_master \
             WHERE type='table' AND sql IS NOT NULL \
             ORDER BY name",
        )
        .unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    for sql in rows.flatten() {
        println!("{};", sql);
    }
}
