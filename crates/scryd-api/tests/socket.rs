#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use scryd_api::{bind, ApiError};
use tempfile::TempDir;

#[tokio::test]
async fn bind_honours_caller_supplied_mode() {
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");

    // v0.3.1 default — anyone-on-host.
    let _listener = bind(&scryd_dir, 0o666).await.expect("bind succeeds");

    let sock = scryd_dir.join("scryd.sock");
    let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o666, "socket file mode = {:o}", mode);
}

#[tokio::test]
async fn bind_creates_socket_at_mode_0660_when_requested() {
    // Recoverable v0.2.0 posture: caller passes 0o660 via [server]
    // socket_mode and gets the group-restricted shape back.
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");

    let _listener = bind(&scryd_dir, 0o660).await.expect("bind succeeds");

    let sock = scryd_dir.join("scryd.sock");
    let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o660, "socket file mode = {:o}", mode);
}

#[tokio::test]
async fn bind_creates_parent_dir_at_mode_0700() {
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");
    assert!(!scryd_dir.exists());

    let _listener = bind(&scryd_dir, 0o666).await.unwrap();

    let mode = std::fs::metadata(&scryd_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "parent dir mode = {:o}", mode);
}

#[tokio::test]
async fn second_bind_against_live_socket_returns_already_running() {
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");

    let _first = bind(&scryd_dir, 0o666).await.unwrap();

    // Give the first listener a moment to be accepting before the second
    // bind tries to connect to it for liveness detection.
    tokio::time::sleep(Duration::from_millis(50)).await;

    match bind(&scryd_dir, 0o666).await {
        Err(ApiError::AlreadyRunning { path }) => {
            assert!(path.ends_with("scryd.sock"));
        }
        other => panic!("expected AlreadyRunning, got {other:?}"),
    }
}

#[tokio::test]
async fn stale_socket_left_by_a_dead_daemon_is_cleaned_up() {
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");
    std::fs::create_dir_all(&scryd_dir).unwrap();
    std::fs::set_permissions(&scryd_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    // Drop a stale file at the socket path. connect() will fail with
    // ENOTSOCK / ECONNREFUSED; the bind path should unlink and rebind.
    let sock = scryd_dir.join("scryd.sock");
    std::fs::write(&sock, b"stale leftover").unwrap();

    let _listener = bind(&scryd_dir, 0o666)
        .await
        .expect("rebind after stale file");
    assert!(sock.exists());
}
