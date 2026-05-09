use std::collections::HashMap;
use std::path::PathBuf;

use scryd::path_resolution::{resolve_config_path_for, resolve_socket_path_for, MapEnv};
use tempfile::TempDir;

fn env(pairs: &[(&str, &str)]) -> MapEnv {
    let mut m = HashMap::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), (*v).to_string());
    }
    MapEnv(m)
}

#[test]
fn config_path_uses_xdg_when_set() {
    let env = env(&[("XDG_CONFIG_HOME", "/var/tmp/xdg")]);
    let p = resolve_config_path_for(1000, &env).unwrap();
    assert_eq!(p, PathBuf::from("/var/tmp/xdg/scryd/config.toml"));
}

#[test]
fn config_path_uses_etc_scryd_when_root_and_xdg_unset() {
    let env = env(&[]);
    let p = resolve_config_path_for(0, &env).unwrap();
    assert_eq!(p, PathBuf::from("/etc/scryd/config.toml"));
}

#[test]
fn config_path_uses_home_when_unprivileged() {
    let env = env(&[("HOME", "/home/alice")]);
    let p = resolve_config_path_for(1000, &env).unwrap();
    assert_eq!(p, PathBuf::from("/home/alice/.config/scryd/config.toml"));
}

#[test]
fn socket_path_falls_back_to_run_scryd_when_xdg_runtime_dir_socket_missing() {
    let runtime_dir = TempDir::new().unwrap();
    let system_dir = TempDir::new().unwrap();
    let system_sock = system_dir.path().join("scryd.sock");
    std::fs::write(&system_sock, b"").unwrap();

    let env = env(&[("XDG_RUNTIME_DIR", runtime_dir.path().to_str().unwrap())]);
    let resolved = resolve_socket_path_for(&env, &system_sock).unwrap();
    assert_eq!(resolved, system_sock);
}
