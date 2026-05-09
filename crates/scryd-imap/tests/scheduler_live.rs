//! Integration test: Scheduler::start spawns a supervisor that
//! connects, backfills, enters IDLE, and picks up a fresh inject.
//! Scheduler::shutdown returns cleanly within budget.

#![cfg(unix)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use scryd_config::Config;
use scryd_imap::{MessageSink, Scheduler};
use support::test_sink::CollectingSink;

fn write_temp_config(toml_body: &str) -> Arc<Config> {
    let path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(path.path(), toml_body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path.path(), std::fs::Permissions::from_mode(0o600))
            .unwrap();
    }
    Arc::new(Config::load(path.path()).expect("config loads"))
}

fn config_for_one_account() -> Arc<Config> {
    let host = support::greenmail::host();
    let port = support::greenmail::imap_port();
    write_temp_config(&format!(
        r#"
[[accounts]]
id = "primary"
host = "{host}"
port = {port}
user = "test"
password = "test"
tls = false
folders = ["INBOX"]
"#
    ))
}

#[tokio::test]
async fn scheduler_starts_one_supervisor_and_picks_up_inject() {
    if support::greenmail::skip_if_unreachable() {
        return;
    }

    let prefix = support::greenmail::unique_prefix();

    let config = config_for_one_account();
    let collecting = Arc::new(CollectingSink::new());
    let sink: Arc<dyn MessageSink> = collecting.clone();

    let scheduler =
        Scheduler::new(config, sink).with_idle_recycle(Duration::from_millis(500));
    scheduler.start().await.expect("start");

    // Wait for the supervisor to connect, backfill, and enter IDLE.
    tokio::time::sleep(Duration::from_millis(800)).await;

    let pre_count = collecting.submitted_count();

    support::greenmail::inject_message(
        "alice@localhost",
        "test@localhost",
        &format!("{prefix}-sched-fresh"),
        "after-scheduler-up",
    )
    .await
    .expect("inject");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while collecting.submitted_count() <= pre_count {
        if tokio::time::Instant::now() >= deadline {
            scheduler.shutdown().await.ok();
            panic!("scheduler did not pick up inject within 5s");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::timeout(Duration::from_secs(5), scheduler.shutdown())
        .await
        .expect("shutdown returns within 5s")
        .expect("shutdown ok");
}

#[tokio::test]
async fn scheduler_marks_health_failure_when_imap_is_unreachable() {
    // Always-on test: doesn't need GreenMail since the point is that
    // a closed port produces a connect-failure health update.

    let config = write_temp_config(
        r#"
[[accounts]]
id = "deadhost"
host = "127.0.0.1"
port = 1
user = "test"
password = "test"
tls = false
folders = ["INBOX"]
"#,
    );
    let collecting = Arc::new(CollectingSink::new());
    let sink: Arc<dyn MessageSink> = collecting.clone();

    let scheduler =
        Scheduler::new(config, sink).with_idle_recycle(Duration::from_millis(500));
    scheduler.start().await.expect("start");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut saw_failure = false;
    while tokio::time::Instant::now() < deadline {
        let any_match = collecting
            .state_updates
            .lock()
            .unwrap()
            .iter()
            .any(|(_, _, u)| {
                matches!(
                    u.account_health.as_deref(),
                    Some("connect-failure") | Some("auth-rejected") | Some("tls-failure")
                )
            });
        if any_match {
            saw_failure = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    scheduler.shutdown().await.ok();
    assert!(
        saw_failure,
        "expected a non-active account_health update within 5s"
    );
}
