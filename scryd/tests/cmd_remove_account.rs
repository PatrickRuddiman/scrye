#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use tempfile::TempDir;

const TWO_ACCOUNTS_CONFIG: &str = r#"[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "p1"
folders = ["INBOX"]

[[accounts]]
id = "work"
host = "imap.work.example.com"
port = 993
user = "alice.work@example.com"
password = "p2"
folders = ["INBOX", "Sent"]
"#;

fn scryd() -> Command {
    Command::cargo_bin("scryd").expect("binary built")
}

fn write_fixture(home: &TempDir) -> std::path::PathBuf {
    let dir = home.path().join(".config/scryd");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, TWO_ACCOUNTS_CONFIG).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

#[test]
fn remove_account_writes_without_explicit_root_check() {
    // v0.3.1: the explicit cli_effective_euid != 0 bail is gone.
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let _ = write_fixture(&home);
    let output = scryd()
        .env("SCRYD_CLI_FAKE_EUID", "1000")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["remove-account", "primary", "--yes"])
        .output()
        .expect("run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("requires root"), "elevation error leaked: {stderr}");
}

#[test]
fn remove_account_for_unknown_account_exits_bad_input() {
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let _ = write_fixture(&home);
    let output = scryd()
        .env("SCRYD_CLI_FAKE_EUID", "0")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["remove-account", "ghost", "--yes"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ghost"), "stderr: {stderr}");
}

#[test]
fn remove_account_with_yes_flag_skips_prompt_and_removes() {
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let path = write_fixture(&home);
    let output = scryd()
        .env("SCRYD_CLI_FAKE_EUID", "0")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["remove-account", "primary", "--yes"])
        .output()
        .expect("run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(!body.contains("id = \"primary\""), "primary not removed: {body}");
    assert!(body.contains("id = \"work\""), "work clobbered: {body}");
    assert!(body.contains("password = \"p2\""), "work password clobbered: {body}");
    assert!(body.contains("user = \"alice.work@example.com\""), "work user clobbered: {body}");
}

#[test]
fn remove_account_without_yes_flag_aborts_on_blank_input() {
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let path = write_fixture(&home);
    let original = std::fs::read_to_string(&path).unwrap();

    let output = scryd()
        .env("SCRYD_CLI_FAKE_EUID", "0")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["remove-account", "primary"])
        .write_stdin("\n")
        .output()
        .expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("aborted"), "stdout: {stdout}");

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, original, "config changed after abort");
}

#[test]
fn remove_account_help_lists_yes_flag() {
    let output = scryd()
        .args(["remove-account", "--help"])
        .output()
        .expect("run");
    let combined = String::from_utf8_lossy(&output.stdout);
    assert!(combined.contains("remove-account"), "{combined}");
    assert!(combined.contains("--yes"), "{combined}");
}
