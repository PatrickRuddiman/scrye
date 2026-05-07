use scryd_config::Config;
use scryd_storage::StorageHandle;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

fn write_cfg(toml: &str) -> Config {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(toml.as_bytes()).unwrap();
    Config::load(f.path()).expect("load cfg")
}

#[tokio::test]
async fn reconcile_inserts_new_accounts() {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 2).unwrap();

    let cfg = write_cfg(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"

[[accounts]]
id = "work"
host = "imap.work.com"
port = 993
user = "alice@work.com"
password = "q"
"#,
    );
    let diff = h.reconcile_from_config(&cfg).await.unwrap();
    assert_eq!(diff.added, vec!["primary".to_string(), "work".to_string()]);
    assert!(diff.updated.is_empty());
    assert!(diff.inactivated.is_empty());

    let listed = h.list_active_accounts().await.unwrap();
    assert_eq!(listed.len(), 2);
}

#[tokio::test]
async fn reconcile_inactivates_removed_accounts() {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 2).unwrap();

    let cfg_two = write_cfg(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"

[[accounts]]
id = "work"
host = "imap.work.com"
port = 993
user = "alice@work.com"
password = "q"
"#,
    );
    h.reconcile_from_config(&cfg_two).await.unwrap();

    let cfg_one = write_cfg(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"
"#,
    );
    let diff = h.reconcile_from_config(&cfg_one).await.unwrap();
    assert!(diff.added.is_empty());
    assert_eq!(diff.inactivated, vec!["work".to_string()]);

    let active = h.list_active_accounts().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].account_id, "primary");

    let all = h.list_all_accounts().await.unwrap();
    assert_eq!(all.len(), 2, "inactivated account should be retained");
    let work = all.iter().find(|a| a.account_id == "work").unwrap();
    assert!(!work.active);
}

#[tokio::test]
async fn reconcile_marks_changed_accounts_updated() {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 2).unwrap();

    let cfg_v1 = write_cfg(
        r#"
[[accounts]]
id = "primary"
host = "imap.old.com"
port = 993
user = "alice@example.com"
password = "p"
"#,
    );
    h.reconcile_from_config(&cfg_v1).await.unwrap();

    let cfg_v2 = write_cfg(
        r#"
[[accounts]]
id = "primary"
host = "imap.new.com"
port = 993
user = "alice@example.com"
password = "p"
"#,
    );
    let diff = h.reconcile_from_config(&cfg_v2).await.unwrap();
    assert_eq!(diff.updated, vec!["primary".to_string()]);

    let active = h.list_active_accounts().await.unwrap();
    assert_eq!(active[0].host, "imap.new.com");
}

#[tokio::test]
async fn reconcile_no_op_when_unchanged() {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 2).unwrap();

    let cfg = write_cfg(
        r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p"
"#,
    );
    h.reconcile_from_config(&cfg).await.unwrap();
    let diff = h.reconcile_from_config(&cfg).await.unwrap();
    assert!(diff.added.is_empty());
    assert!(diff.updated.is_empty());
    assert!(diff.inactivated.is_empty());
}

#[tokio::test]
async fn no_password_column_in_accounts_table() {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 1).unwrap();
    let cols: Vec<String> = h
        .with_reader(|conn| {
            let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('accounts')")?;
            let names: Result<Vec<String>, _> = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect();
            names.map_err(Into::into)
        })
        .await
        .unwrap();
    assert!(
        !cols.iter().any(|c| c.contains("password")),
        "accounts schema must never contain a password column; got {cols:?}"
    );
}
