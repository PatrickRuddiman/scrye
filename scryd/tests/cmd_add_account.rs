#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use assert_cmd::Command;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Notify;

const RECONCILE_OK_BODY: &str = r#"{"started":true,"accounts_added":["primary"],"accounts_updated":[],"accounts_inactivated":[]}"#;

async fn spawn_fake_reconcile(body: &'static str) -> (TempDir, Arc<Notify>) {
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");
    std::fs::create_dir_all(&scryd_dir).unwrap();
    let socket: PathBuf = scryd_dir.join("scryd.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let stop = Arc::new(Notify::new());
    let stop_clone = stop.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_clone.notified() => return,
                accept = listener.accept() => {
                    let Ok((mut stream, _)) = accept else { continue };
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 4096];
                        let _ = tokio::time::timeout(
                            Duration::from_millis(500),
                            stream.read(&mut buf),
                        )
                        .await;
                        let response = format!(
                            "HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        let _ = stream.shutdown().await;
                    });
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (dir, stop)
}

fn scryd() -> Command {
    Command::cargo_bin("scryd").expect("binary built")
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

#[test]
fn flag_path_writes_config_and_dispatches_reconcile() {
    block_on(async {
        let (runtime_dir, _stop) = spawn_fake_reconcile(RECONCILE_OK_BODY).await;
        let home = TempDir::new().unwrap();
        let runtime_path = runtime_dir.path().to_path_buf();
        let home_path = home.path().to_path_buf();

        let output = tokio::task::spawn_blocking(move || {
            scryd()
                .env("XDG_RUNTIME_DIR", &runtime_path)
                .env("HOME", &home_path)
                .env_remove("XDG_CONFIG_HOME")
                .args([
                    "add-account",
                    "--account-id",
                    "primary",
                    "--host",
                    "imap.example.com",
                    "--port",
                    "993",
                    "--user",
                    "alice@example.com",
                    "--password-stdin",
                    "--folders",
                    "INBOX",
                ])
                .write_stdin("hunter2\n")
                .output()
                .expect("run")
        })
        .await
        .unwrap();

        assert!(
            output.status.success(),
            "scryd add-account should succeed; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("saved to"), "{stdout}");
        assert!(stdout.contains("picked up by daemon"), "{stdout}");

        let config_path = home.path().join(".config/scryd/config.toml");
        assert!(config_path.exists(), "config.toml must exist");
        let mode =
            std::fs::metadata(&config_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "config mode = {:o}", mode);
        let body = std::fs::read_to_string(&config_path).unwrap();
        assert!(body.contains("id = \"primary\""), "{body}");
        assert!(body.contains("password = \"hunter2\""), "{body}");
        assert!(body.contains("host = \"imap.example.com\""), "{body}");
    });
}

#[test]
fn rotation_updates_password_and_preserves_operator_comment() {
    let runtime = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    // Pre-write a config with an operator comment + an existing account.
    let config_dir = home.path().join(".config/scryd");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    let initial = r#"# operator-authored comment
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "old-password"
folders = ["INBOX"]
"#;
    std::fs::write(&config_path, initial).unwrap();
    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).unwrap();

    // Re-run add-account for the same id with a new password.
    let output = scryd()
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("HOME", home.path())
        .env_remove("XDG_CONFIG_HOME")
        .args([
            "add-account",
            "--account-id",
            "primary",
            "--host",
            "imap.example.com",
            "--user",
            "alice@example.com",
            "--password-stdin",
            "--folders",
            "INBOX",
        ])
        .write_stdin("new-password\n")
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "rotation should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let body = std::fs::read_to_string(&config_path).unwrap();
    assert!(body.contains("operator-authored comment"), "comment lost: {body}");
    assert!(body.contains("password = \"new-password\""), "{body}");
    assert!(!body.contains("\"old-password\""), "old password leaked: {body}");
    // No duplicate account block.
    let count = body.matches("[[accounts]]").count();
    assert_eq!(count, 1, "expected exactly one [[accounts]] block: {body}");
}

#[test]
fn missing_required_flag_in_non_interactive_path_exits_bad_input() {
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let output = scryd()
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args([
            "add-account",
            "--account-id",
            "primary",
            "--host",
            "imap.example.com",
            "--user",
            "alice@example.com",
            "--password-stdin",
        ])
        .write_stdin("") // empty password through stdin
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(4),
        "expected BadInput exit code; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn invalid_account_id_regex_rejected_with_bad_input() {
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let output = scryd()
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args([
            "add-account",
            "--account-id",
            "HasUpper",
            "--host",
            "imap.example.com",
            "--user",
            "alice@example.com",
            "--password-stdin",
        ])
        .write_stdin("hunter2\n")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("HasUpper"), "{stderr}");
}

#[test]
fn add_account_writes_config_without_explicit_root_check() {
    // A non-root invocation that has write access to the resolved
    // config path (here: a tempdir under HOME) succeeds; chown to
    // scryd is best-effort and silently no-ops when not running as
    // root.
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let output = scryd()
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args([
            "add-account",
            "--account-id",
            "primary",
            "--host",
            "imap.example.com",
            "--user",
            "alice@example.com",
            "--password-stdin",
            "--folders",
            "INBOX",
        ])
        .write_stdin("hunter2\n")
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("requires root"), "elevation error leaked: {stderr}");

    let config_path = home.path().join(".config/scryd/config.toml");
    assert!(config_path.exists(), "config.toml must exist");
}

#[test]
fn add_account_with_root_writes_config_and_prints_restart_hint() {
    block_on(async {
        let (runtime_dir, _stop) = spawn_fake_reconcile(RECONCILE_OK_BODY).await;
        let home = TempDir::new().unwrap();
        let runtime_path = runtime_dir.path().to_path_buf();
        let home_path = home.path().to_path_buf();

        let output = tokio::task::spawn_blocking(move || {
            scryd()
                .env("XDG_RUNTIME_DIR", &runtime_path)
                .env("HOME", &home_path)
                .env_remove("XDG_CONFIG_HOME")
                .args([
                    "add-account",
                    "--account-id",
                    "primary",
                    "--host",
                    "imap.example.com",
                    "--user",
                    "alice@example.com",
                    "--password-stdin",
                    "--folders",
                    "INBOX",
                ])
                .write_stdin("hunter2\n")
                .output()
                .expect("run")
        })
        .await
        .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("picked up by daemon"), "{stdout}");
    });
}

#[test]
fn add_account_with_root_and_no_daemon_prints_start_hint() {
    let runtime = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    let output = scryd()
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("HOME", home.path())
        .env_remove("XDG_CONFIG_HOME")
        .args([
            "add-account",
            "--account-id",
            "primary",
            "--host",
            "imap.example.com",
            "--user",
            "alice@example.com",
            "--password-stdin",
            "--folders",
            "INBOX",
        ])
        .write_stdin("hunter2\n")
        .output()
        .expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sudo systemctl start scryd"), "{stdout}");
}
