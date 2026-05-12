use scryd_runtime::xdg;
use scryd_runtime::RuntimeError;
use serial_test::serial;

// Each test mutates process-global env vars. `#[serial]` keeps them on a
// shared lock with the preflight tests, which touch the same vars.

fn clear_xdg_env() {
    for key in [
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "HOME",
    ] {
        std::env::remove_var(key);
    }
}

#[test]
#[serial]
fn runtime_dir_requires_xdg_runtime_dir() {
    clear_xdg_env();
    match xdg::runtime_dir() {
        Err(RuntimeError::MissingRuntimeDir) => {}
        other => panic!("expected MissingRuntimeDir, got {other:?}"),
    }
}

#[test]
#[serial]
fn runtime_dir_appends_scryd() {
    clear_xdg_env();
    std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1001");
    let p = xdg::runtime_dir().unwrap();
    assert_eq!(p, std::path::PathBuf::from("/run/user/1001/scryd"));
}

#[test]
#[serial]
fn config_path_uses_xdg_config_home_when_set() {
    clear_xdg_env();
    std::env::set_var("XDG_CONFIG_HOME", "/home/alice/cfg");
    let p = xdg::config_path().unwrap();
    assert_eq!(
        p,
        std::path::PathBuf::from("/home/alice/cfg/scryd/config.toml")
    );
}

#[test]
#[serial]
fn config_path_falls_back_to_home_dot_config() {
    clear_xdg_env();
    std::env::set_var("HOME", "/home/alice");
    let p = xdg::config_path().unwrap();
    assert_eq!(
        p,
        std::path::PathBuf::from("/home/alice/.config/scryd/config.toml")
    );
}

#[test]
#[serial]
fn data_dir_uses_xdg_data_home_when_set() {
    clear_xdg_env();
    std::env::set_var("XDG_DATA_HOME", "/home/alice/data");
    let p = xdg::data_dir().unwrap();
    assert_eq!(p, std::path::PathBuf::from("/home/alice/data/scryd"));
}

#[test]
#[serial]
fn data_dir_falls_back_to_home_dot_local_share() {
    clear_xdg_env();
    std::env::set_var("HOME", "/home/alice");
    let p = xdg::data_dir().unwrap();
    assert_eq!(
        p,
        std::path::PathBuf::from("/home/alice/.local/share/scryd")
    );
}

#[test]
#[serial]
fn assets_dir_is_data_dir_plus_assets() {
    clear_xdg_env();
    std::env::set_var("XDG_DATA_HOME", "/home/alice/data");
    let p = xdg::assets_dir().unwrap();
    assert_eq!(
        p,
        std::path::PathBuf::from("/home/alice/data/scryd/assets")
    );
}

#[test]
#[serial]
fn empty_string_env_treated_as_unset() {
    clear_xdg_env();
    std::env::set_var("XDG_RUNTIME_DIR", "");
    match xdg::runtime_dir() {
        Err(RuntimeError::MissingRuntimeDir) => {}
        other => panic!("expected MissingRuntimeDir for empty env, got {other:?}"),
    }
}
