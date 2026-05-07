use std::io::Write;

use scryd_config::{AccountPassword, Config, ConfigError};
use tempfile::NamedTempFile;

fn write_config(contents: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("tempfile");
    f.write_all(contents.as_bytes()).expect("write");
    f
}

const MIN_VALID: &str = r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "hunter2"
"#;

#[test]
fn parses_minimal_valid_config() {
    let f = write_config(MIN_VALID);
    let cfg = Config::load(f.path()).expect("loads");

    assert_eq!(cfg.sync.poll_interval_seconds, 300);
    assert!(cfg.sync.use_idle);
    assert_eq!(cfg.sync.folders, vec!["INBOX".to_string()]);
    assert!(cfg.indexers.semantic);

    assert_eq!(cfg.accounts.len(), 1);
    let acc = &cfg.accounts[0];
    assert_eq!(acc.id, "primary");
    assert_eq!(acc.host, "imap.example.com");
    assert_eq!(acc.port, 993);
    assert_eq!(acc.user, "alice@example.com");
    assert_eq!(acc.password.expose(), "hunter2");
    assert!(acc.folders.is_none());
}

#[test]
fn rejects_invalid_account_id_uppercase() {
    let f = write_config(
        r#"
[[accounts]]
id = "HasUpper"
host = "x"
port = 993
user = "a"
password = "p"
"#,
    );
    match Config::load(f.path()) {
        Err(ConfigError::InvalidAccountId(id)) => assert_eq!(id, "HasUpper"),
        other => panic!("expected InvalidAccountId, got {other:?}"),
    }
}

#[test]
fn rejects_invalid_account_id_special_chars() {
    let f = write_config(
        r#"
[[accounts]]
id = "with space"
host = "x"
port = 993
user = "a"
password = "p"
"#,
    );
    assert!(matches!(
        Config::load(f.path()),
        Err(ConfigError::InvalidAccountId(_))
    ));
}

#[test]
fn rejects_port_zero() {
    let f = write_config(
        r#"
[[accounts]]
id = "primary"
host = "x"
port = 0
user = "a"
password = "p"
"#,
    );
    assert!(matches!(
        Config::load(f.path()),
        Err(ConfigError::InvalidPort(_, 0))
    ));
}

#[test]
fn rejects_port_above_u16_max() {
    let f = write_config(
        r#"
[[accounts]]
id = "primary"
host = "x"
port = 65536
user = "a"
password = "p"
"#,
    );
    // u16 deserialization fails before our validate() runs, surfacing as Parse.
    assert!(matches!(Config::load(f.path()), Err(ConfigError::Parse(_, _))));
}

#[test]
fn rejects_duplicate_account_ids() {
    let f = write_config(
        r#"
[[accounts]]
id = "primary"
host = "x"
port = 993
user = "a"
password = "p"

[[accounts]]
id = "primary"
host = "y"
port = 993
user = "b"
password = "q"
"#,
    );
    match Config::load(f.path()) {
        Err(ConfigError::DuplicateAccountId(id)) => assert_eq!(id, "primary"),
        other => panic!("expected DuplicateAccountId, got {other:?}"),
    }
}

#[test]
fn rejects_account_with_empty_folders_when_default_also_empty() {
    let f = write_config(
        r#"
[sync]
folders = []

[[accounts]]
id = "primary"
host = "x"
port = 993
user = "a"
password = "p"
folders = []
"#,
    );
    assert!(matches!(
        Config::load(f.path()),
        Err(ConfigError::NoFolders(_))
    ));
}

#[test]
fn falls_back_to_default_folders_when_override_absent() {
    let f = write_config(MIN_VALID);
    let cfg = Config::load(f.path()).expect("loads");
    let folders_for_account = cfg.accounts[0]
        .folders
        .as_deref()
        .unwrap_or(cfg.sync.folders.as_slice());
    assert_eq!(folders_for_account, &["INBOX"]);
}

#[test]
fn returns_not_found_for_missing_path() {
    let path = std::path::PathBuf::from("/nonexistent/absolutely/not/here.toml");
    match Config::load(&path) {
        Err(ConfigError::NotFound(p)) => assert_eq!(p, path),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn account_password_debug_prints_redacted() {
    let pw = AccountPassword::new("super-secret".to_string());
    let dbg = format!("{pw:?}");
    assert!(dbg.contains("[REDACTED]"), "Debug output: {dbg}");
    assert!(!dbg.contains("super-secret"), "secret leaked: {dbg}");
}

#[test]
fn account_password_expose_returns_original() {
    let pw = AccountPassword::new("rotated-pw".to_string());
    assert_eq!(pw.expose(), "rotated-pw");
}

#[test]
fn empty_config_uses_defaults() {
    let f = write_config("");
    let cfg = Config::load(f.path()).expect("loads");
    assert_eq!(cfg.sync.poll_interval_seconds, 300);
    assert!(cfg.sync.use_idle);
    assert_eq!(cfg.sync.folders, vec!["INBOX".to_string()]);
    assert!(cfg.indexers.semantic);
    assert!(cfg.accounts.is_empty());
}
