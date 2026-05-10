use assert_cmd::Command;

fn scryd() -> Command {
    Command::cargo_bin("scryd").expect("binary built")
}

#[test]
fn reindex_without_xdg_runtime_dir_falls_through_to_system_socket_path() {
    // v0.2.0: when XDG_RUNTIME_DIR is unset, UdsClient::from_env falls
    // back to /run/scryd/scryd.sock. On a host with no daemon (CI
    // runners, dev boxes) that path doesn't exist, so the call surfaces
    // ExitCode::DaemonNotRunning (2) rather than a generic env-var error.
    let output = scryd()
        .env_remove("XDG_RUNTIME_DIR")
        .arg("reindex")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains("daemon-not-running"),
        "expected daemon-not-running category: {stderr}"
    );
}

#[test]
fn reindex_with_unset_runtime_dir_returns_clear_error_message() {
    let output = scryd()
        .env_remove("XDG_RUNTIME_DIR")
        .arg("reindex")
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(stderr.starts_with("scryd:"), "stderr starts with scryd:; got: {stderr}");
}

#[test]
fn reindex_with_dead_socket_path_exits_daemon_not_running_code() {
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap();
    // No socket file at the resolved path.
    let output = scryd()
        .env("XDG_RUNTIME_DIR", dir.path())
        .arg("reindex")
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected DaemonNotRunning (exit 2) when no socket file present"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains("daemon-not-running"),
        "expected daemon-not-running category in stderr: {stderr}"
    );
}
