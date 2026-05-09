use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use crate::secret::AccountPassword;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Reserved. The `[server]` section is empty in v1; XDG-derived paths and
    /// the kernel-enforced socket location replace what would have been bind
    /// options. Future versions may introduce knobs here.
    #[serde(default)]
    pub server: ServerCfg,
    #[serde(default)]
    pub sync: SyncCfg,
    #[serde(default)]
    pub indexers: IndexersCfg,
    #[serde(default)]
    pub accounts: Vec<AccountCfg>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ServerCfg {}

#[derive(Debug, Deserialize)]
pub struct SyncCfg {
    #[serde(default = "SyncCfg::default_poll_interval")]
    pub poll_interval_seconds: u32,
    #[serde(default = "SyncCfg::default_use_idle")]
    pub use_idle: bool,
    #[serde(default = "SyncCfg::default_folders")]
    pub folders: Vec<String>,
}

impl SyncCfg {
    fn default_poll_interval() -> u32 {
        300
    }
    fn default_use_idle() -> bool {
        true
    }
    fn default_folders() -> Vec<String> {
        vec!["INBOX".to_string()]
    }
}

impl Default for SyncCfg {
    fn default() -> Self {
        Self {
            poll_interval_seconds: Self::default_poll_interval(),
            use_idle: Self::default_use_idle(),
            folders: Self::default_folders(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct IndexersCfg {
    #[serde(default = "IndexersCfg::default_semantic")]
    pub semantic: bool,
}

impl IndexersCfg {
    fn default_semantic() -> bool {
        true
    }
}

impl Default for IndexersCfg {
    fn default() -> Self {
        Self {
            semantic: Self::default_semantic(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountCfg {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: AccountPassword,
    #[serde(default)]
    pub folders: Option<Vec<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found at {0}")]
    NotFound(PathBuf),
    #[error("read config {0}: {1}")]
    Read(PathBuf, #[source] std::io::Error),
    #[error("parse config {0}: {1}")]
    Parse(PathBuf, #[source] toml::de::Error),
    #[error("invalid account id `{0}`: must match {ACCOUNT_ID_PATTERN}")]
    InvalidAccountId(String),
    #[error("account `{0}` has invalid port {1}: must be 1..=65535")]
    InvalidPort(String, u16),
    #[error("account `{0}` has no folders to sync (override is empty and [sync].folders is empty)")]
    NoFolders(String),
    #[error("duplicate account id `{0}`")]
    DuplicateAccountId(String),
    #[error("config file {path} mode is too permissive: expected 0600, got {mode_seen:o}")]
    PermissionInvariant { path: PathBuf, mode_seen: u32 },
    #[error("XDG_CONFIG_HOME and HOME are both unset; cannot resolve config path")]
    XdgUnresolvable,
}

const ACCOUNT_ID_PATTERN: &str = "^[a-z0-9_-]+$";

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ConfigError::NotFound(path.to_path_buf()));
            }
            Err(e) => return Err(ConfigError::Read(path.to_path_buf(), e)),
        };
        crate::permissions::assert_mode_0600(path)?;
        let cfg: Config = toml::from_str(&raw)
            .map_err(|e| ConfigError::Parse(path.to_path_buf(), e))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load_from_xdg() -> Result<Self, ConfigError> {
        let path = xdg_config_path()?;
        Self::load(&path)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let id_re = Regex::new(ACCOUNT_ID_PATTERN).expect("valid regex");
        let mut seen: HashSet<&str> = HashSet::new();

        for account in &self.accounts {
            if !id_re.is_match(&account.id) {
                return Err(ConfigError::InvalidAccountId(account.id.clone()));
            }
            if account.port == 0 {
                return Err(ConfigError::InvalidPort(account.id.clone(), account.port));
            }
            let folders_for_account = account
                .folders
                .as_deref()
                .unwrap_or(self.sync.folders.as_slice());
            if folders_for_account.is_empty() {
                return Err(ConfigError::NoFolders(account.id.clone()));
            }
            if !seen.insert(account.id.as_str()) {
                return Err(ConfigError::DuplicateAccountId(account.id.clone()));
            }
        }
        Ok(())
    }
}

fn xdg_config_path() -> Result<PathBuf, ConfigError> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir).join("scryd").join("config.toml"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Ok(PathBuf::from(home)
                .join(".config")
                .join("scryd")
                .join("config.toml"));
        }
    }
    Err(ConfigError::XdgUnresolvable)
}
