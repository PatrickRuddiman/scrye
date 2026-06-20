#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use scryd_runtime::{run_preflight, RuntimeError};
use serial_test::serial;
use tempfile::TempDir;

fn setup_layout(config_mode: u32, data_dir_mode: Option<u32>) -> (TempDir, TempDir) {
    let config_root = TempDir::new().unwrap();
    let data_root = TempDir::new().unwrap();

    // Place config.toml at $XDG_CONFIG_HOME/scryd/config.toml.
    let config_dir = config_root.path().join("scryd");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_file = config_dir.join("config.toml");
    std::fs::write(&config_file, "").unwrap();
    std::fs::set_permissions(&config_file, std::fs::Permissions::from_mode(config_mode)).unwrap();

    // Optionally pre-create the data dir with a specific mode.
    if let Some(mode) = data_dir_mode {
        let dd = data_root.path().join("scryd");
        std::fs::create_dir_all(&dd).unwrap();
        std::fs::set_permissions(&dd, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    std::env::set_var("XDG_CONFIG_HOME", config_root.path());
    std::env::set_var("XDG_DATA_HOME", data_root.path());
    std::env::remove_var("HOME");

    (config_root, data_root)
}

fn cleanup_env() {
    for k in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "HOME"] {
        std::env::remove_var(k);
    }
}

#[test]
#[serial]
fn preflight_passes_when_invariants_hold() {
    let _g = setup_layout(0o600, None);

    let ok = run_preflight().expect("invariants hold");
    assert!(ok.config_path.ends_with("scryd/config.toml"));
    assert!(ok.data_dir.ends_with("scryd"));
    assert!(ok.assets_dir.ends_with("scryd/assets"));
    cleanup_env();
}

#[test]
#[serial]
fn preflight_creates_data_dir_when_missing() {
    let _g = setup_layout(0o600, None);
    let ok = run_preflight().unwrap();
    assert!(ok.data_dir.is_dir());
    let mode = std::fs::metadata(&ok.data_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    cleanup_env();
}

#[test]
#[serial]
fn preflight_rejects_world_readable_config() {
    let _g = setup_layout(0o644, None);
    match run_preflight() {
        Err(RuntimeError::PermissionInvariant { reason, path }) => {
            assert!(reason.contains("config"), "reason={reason}");
            assert!(reason.contains("world"), "reason={reason}");
            assert!(path.ends_with("config.toml"));
        }
        other => panic!("expected PermissionInvariant, got {other:?}"),
    }
    cleanup_env();
}

#[test]
#[serial]
fn preflight_accepts_group_readable_config() {
    // v0.3.1 installs config as scryd:scryd 0640 so members of group
    // scryd can read it; preflight must accept that.
    let _g = setup_layout(0o640, None);
    run_preflight().expect("0o640 config should pass preflight");
    cleanup_env();
}

#[test]
#[serial]
fn preflight_rejects_loose_data_dir() {
    let _g = setup_layout(0o600, Some(0o755));
    match run_preflight() {
        Err(RuntimeError::PermissionInvariant { reason, .. }) => {
            assert!(reason.contains("data"));
        }
        other => panic!("expected PermissionInvariant for loose data dir, got {other:?}"),
    }
    cleanup_env();
}
