#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use tempfile::TempDir;

const TWO_ACCOUNTS_CONFIG: &str = r#"# operator-authored comment
[[accounts]]
id = "primary"
host = "imap.example.com"
port = 993
user = "alice@example.com"
password = "old-password"
folders = ["INBOX"]

[[accounts]]
id = "work"
host = "imap.work.example.com"
port = 993
user = "alice.work@example.com"
password = "work-password"
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
fn rotate_password_writes_without_explicit_root_check() {
    // v0.3.1: the explicit cli_effective_euid != 0 bail is gone.
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let _ = write_fixture(&home);
    let output = scryd()
        .env("SCRYD_CLI_FAKE_EUID", "1000")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["rotate-password", "primary", "--password-stdin"])
        .write_stdin("new-pw\n")
        .output()
        .expect("run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("requires root"), "elevation error leaked: {stderr}");
}

#[test]
fn rotate_password_for_unknown_account_exits_bad_input() {
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let _ = write_fixture(&home);
    let output = scryd()
        .env("SCRYD_CLI_FAKE_EUID", "0")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["rotate-password", "ghost", "--password-stdin"])
        .write_stdin("new-pw\n")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ghost"), "stderr: {stderr}");
}

#[test]
fn rotate_password_for_existing_account_updates_field_and_preserves_rest() {
    let home = TempDir::new().unwrap();
    let runtime = TempDir::new().unwrap();
    let path = write_fixture(&home);
    let output = scryd()
        .env("SCRYD_CLI_FAKE_EUID", "0")
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["rotate-password", "primary", "--password-stdin"])
        .write_stdin("rotated-pw\n")
        .output()
        .expect("run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("password = \"rotated-pw\""), "{body}");
    assert!(!body.contains("\"old-password\""), "old-password leaked: {body}");
    assert!(body.contains("operator-authored comment"), "comment lost: {body}");
    assert!(body.contains("password = \"work-password\""), "work pw clobbered: {body}");
    assert!(body.contains("user = \"alice.work@example.com\""), "work user clobbered: {body}");
}

#[test]
fn rotate_password_help_lists_only_the_verb_no_extra_flags() {
    let output = scryd()
        .args(["rotate-password", "--help"])
        .output()
        .expect("run");
    let combined = String::from_utf8_lossy(&output.stdout);
    assert!(combined.contains("rotate-password"), "{combined}");
    assert!(combined.contains("--password-stdin"), "{combined}");
    assert!(combined.contains("<ACCOUNT_ID>"), "{combined}");
}
