use std::sync::Arc;
use std::time::Duration;

use scryd_config::Config;
use scryd_imap::{
    backoff_after, BACKOFF_CAP, INITIAL_BACKOFF, MAX_CONNECTIONS_PER_ACCOUNT,
};
use scryd_imap::credential::{ConfigCredentialFetcher, CredentialFetcher};

#[test]
fn first_failure_returns_initial_backoff() {
    assert_eq!(backoff_after(0), INITIAL_BACKOFF);
}

#[test]
fn backoff_doubles_each_attempt() {
    assert_eq!(backoff_after(0), Duration::from_secs(30));
    assert_eq!(backoff_after(1), Duration::from_secs(60));
    assert_eq!(backoff_after(2), Duration::from_secs(120));
    assert_eq!(backoff_after(3), Duration::from_secs(240));
    assert_eq!(backoff_after(4), Duration::from_secs(480));
    assert_eq!(backoff_after(5), Duration::from_secs(960));
    assert_eq!(backoff_after(6), Duration::from_secs(1920));
}

#[test]
fn backoff_caps_at_one_hour() {
    assert_eq!(backoff_after(7), BACKOFF_CAP);
    assert_eq!(backoff_after(20), BACKOFF_CAP);
    assert_eq!(backoff_after(100), BACKOFF_CAP);
}

#[test]
fn constants_match_slice() {
    assert_eq!(MAX_CONNECTIONS_PER_ACCOUNT, 5);
    assert_eq!(INITIAL_BACKOFF, Duration::from_secs(30));
    assert_eq!(BACKOFF_CAP, Duration::from_secs(3600));
}

#[test]
fn config_credential_fetcher_returns_password_for_known_account() {
    let toml = r#"
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "hunter2"
"#;
    let mut tf = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write as _;
    tf.write_all(toml.as_bytes()).unwrap();

    let cfg = Arc::new(Config::load(tf.path()).unwrap());
    let fetcher = ConfigCredentialFetcher::new(cfg);

    assert_eq!(fetcher.fetch("primary").as_deref(), Some("hunter2"));
    assert!(fetcher.fetch("ghost").is_none());
}
