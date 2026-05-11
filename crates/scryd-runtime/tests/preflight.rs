#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use scryd_runtime::{run_preflight, RuntimeError};
use tempfile::TempDir;

fn setup_layout(
    runtime_dir_mode: u32,
    config_mode: u32,
    data_dir_mode: Option<u32>,
) -> (TempDir, TempDir, TempDir) {
    let runtime = TempDir::new().unwrap();
    let config_root = TempDir::new().unwrap();
    let data_root = TempDir::new().unwrap();

    // $XDG_RUNTIME_DIR/scryd doesn't need to exist for preflight; we test
    // the parent. Set the parent's mode.
    std::fs::set_permissions(
        runtime.path(),
        std::fs::Permissions::from_mode(runtime_dir_mode),
    )
    .unwrap();

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

    std::env::set_var("XDG_RUNTIME_DIR", runtime.path());
    std::env::set_var("XDG_CONFIG_HOME", config_root.path());
    std::env::set_var("XDG_DATA_HOME", data_root.path());
    std::env::remove_var("HOME");

    (runtime, config_root, data_root)
}

fn cleanup_env() {
    for k in ["XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "HOME"] {
        std::env::remove_var(k);
    }
}

#[test]
fn preflight_passes_when_invariants_hold() {
    let _g = (
        setup_layout(0o700, 0o600, None),
    );

    let ok = run_preflight().expect("invariants hold");
    assert!(ok.config_path.ends_with("scryd/config.toml"));
    assert!(ok.data_dir.ends_with("scryd"));
    assert!(ok.assets_dir.ends_with("scryd/assets"));
    cleanup_env();
}

#[test]
fn preflight_creates_data_dir_when_missing() {
    let _g = setup_layout(0o700, 0o600, None);
    let ok = run_preflight().unwrap();
    assert!(ok.data_dir.is_dir());
    let mode = std::fs::metadata(&ok.data_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    cleanup_env();
}

#[test]
fn preflight_rejects_world_readable_config() {
    let _g = setup_layout(0o700, 0o644, None);
    match run_preflight() {
        Err(RuntimeError::PermissionInvariant { reason, path }) => {
            assert!(reason.contains("config"));
            assert!(path.ends_with("config.toml"));
        }
        other => panic!("expected PermissionInvariant, got {other:?}"),
    }
    cleanup_env();
}

#[test]
fn preflight_accepts_loose_runtime_dir() {
    // Preflight asserts the runtime dir is a directory but no longer
    // asserts an exact mode; the install-time tmpfiles drop-in is the
    // source of truth (v0.3.1 ships 0755 scryd:scryd).
    let _g = setup_layout(0o755, 0o600, None);
    match run_preflight() {
        Ok(_) => {}
        other => panic!("expected preflight to accept loose runtime dir, got {other:?}"),
    }
    cleanup_env();
}

#[test]
fn preflight_rejects_loose_data_dir() {
    let _g = setup_layout(0o700, 0o600, Some(0o755));
    match run_preflight() {
        Err(RuntimeError::PermissionInvariant { reason, .. }) => {
            assert!(reason.contains("data"));
        }
        other => panic!("expected PermissionInvariant for loose data dir, got {other:?}"),
    }
    cleanup_env();
}

#[test]
fn preflight_fails_without_xdg_runtime_dir() {
    cleanup_env();
    let runtime_root = TempDir::new().unwrap();
    let _ = runtime_root;
    match run_preflight() {
        Err(RuntimeError::MissingRuntimeDir) => {}
        other => panic!("expected MissingRuntimeDir, got {other:?}"),
    }
    let _: PathBuf;
}
