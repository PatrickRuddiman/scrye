use scryd_runtime::xdg;
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
    // An empty XDG var must be ignored, falling through to the HOME default
    // rather than resolving to a bogus root-relative path.
    clear_xdg_env();
    std::env::set_var("XDG_DATA_HOME", "");
    std::env::set_var("HOME", "/home/alice");
    let p = xdg::data_dir().unwrap();
    assert_eq!(
        p,
        std::path::PathBuf::from("/home/alice/.local/share/scryd")
    );
}
