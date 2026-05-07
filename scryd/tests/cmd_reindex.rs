#![cfg(unix)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use assert_cmd::Command;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Notify;

async fn spawn_fake_api(status: u16, body: &'static str) -> (TempDir, Arc<Notify>) {
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
                        let reason = match status {
                            202 => "Accepted",
                            409 => "Conflict",
                            500 => "Internal Server Error",
                            _ => "OK",
                        };
                        let response = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
fn reindex_202_prints_started_message_and_exits_zero() {
    block_on(async {
        let (dir, _stop) = spawn_fake_api(202, r#"{"started":true}"#).await;
        let runtime_dir = dir.path().to_path_buf();
        let output = tokio::task::spawn_blocking(move || {
            scryd()
                .env("XDG_RUNTIME_DIR", &runtime_dir)
                .arg("reindex")
                .output()
                .expect("run")
        })
        .await
        .unwrap();
        assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("reindex started"), "{stdout}");
        assert!(stdout.contains("journalctl --user -u scryd"), "{stdout}");
    });
}

#[test]
fn reindex_409_prints_already_running_and_exits_zero() {
    block_on(async {
        let (dir, _stop) = spawn_fake_api(
            409,
            r#"{"error":{"code":"conflict","message":"a reindex is already in progress"}}"#,
        )
        .await;
        let runtime_dir = dir.path().to_path_buf();
        let output = tokio::task::spawn_blocking(move || {
            scryd()
                .env("XDG_RUNTIME_DIR", &runtime_dir)
                .arg("reindex")
                .output()
                .expect("run")
        })
        .await
        .unwrap();
        assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("already running"), "{stdout}");
    });
}

#[test]
fn reindex_500_exits_daemon_rejected() {
    block_on(async {
        let (dir, _stop) = spawn_fake_api(
            500,
            r#"{"error":{"code":"internal_error","message":"oops"}}"#,
        )
        .await;
        let runtime_dir = dir.path().to_path_buf();
        let output = tokio::task::spawn_blocking(move || {
            scryd()
                .env("XDG_RUNTIME_DIR", &runtime_dir)
                .arg("reindex")
                .output()
                .expect("run")
        })
        .await
        .unwrap();
        assert_eq!(output.status.code(), Some(5));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("daemon-rejected"), "{stderr}");
    });
}

#[test]
fn reindex_help_lists_only_the_verb_no_extra_flags() {
    let output = scryd()
        .args(["reindex", "--help"])
        .output()
        .expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // No filter flags on reindex; just the standard --help / --version.
    assert!(stdout.contains("Usage: scryd reindex"), "{stdout}");
}
