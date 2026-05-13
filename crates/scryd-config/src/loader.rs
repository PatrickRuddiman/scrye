use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use crate::secret::AccountPassword;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Server-side knobs. Defaults to "open API, peercred check off"
    /// — the v0.3.1 service shape. See [`ServerCfg`].
    #[serde(default)]
    pub server: ServerCfg,
    #[serde(default)]
    pub sync: SyncCfg,
    #[serde(default)]
    pub indexers: IndexersCfg,
    #[serde(default)]
    pub accounts: Vec<AccountCfg>,
}

#[derive(Debug, Deserialize)]
pub struct ServerCfg {
    /// Whether to enforce the SO_PEERCRED uid match at accept time.
    /// Defaults to `false` (open) — the consumer's higher-layer API
    /// is the auth boundary. Set to `true` to accept only the
    /// daemon's own uid (same-uid enforcement).
    #[serde(default = "default_require_peer_uid")]
    pub require_peer_uid: bool,
    /// File mode applied to `/run/scryd/scryd.sock` after bind. The
    /// kernel rejects connections whose euid + group don't satisfy
    /// the mode bits, so this is the coarse network-access gate.
    /// Defaults to `0o666` (anyone on the host). Override to `0o660`
    /// + manage the directory's group for kernel-level gating.
    #[serde(default = "default_socket_mode")]
    pub socket_mode: u32,
}

fn default_require_peer_uid() -> bool {
    false
}

fn default_socket_mode() -> u32 {
    0o666
}

impl Default for ServerCfg {
    fn default() -> Self {
        Self {
            require_peer_uid: default_require_peer_uid(),
            socket_mode: default_socket_mode(),
        }
    }
}

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
    /// Whether to wrap the IMAP session in TLS (the production path).
    /// Defaults to `true`; set to `false` only for local test fixtures
    /// (e.g. GreenMail) that speak plain IMAP. This is a TLS-or-not
    /// toggle, not a cert-skip-verify option.
    #[serde(default = "default_tls")]
    pub tls: bool,
    /// Optional PEM file path with one or more X.509 root certificates
    /// scryd should use as trust anchors for THIS account's TLS
    /// handshake instead of the baked-in `webpki-roots` bundle.
    /// Defaults to `None` (use system / baked-in roots). Useful for
    /// corporate IMAP servers behind a private CA without rebuilding
    /// scryd against a custom rustls trust store.
    #[serde(default)]
    pub tls_ca_path: Option<PathBuf>,
}

fn default_tls() -> bool {
    true
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
    #[error("config file {path} mode is too permissive: world bits set on {mode_seen:o}")]
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
        crate::permissions::assert_no_world_bits(path)?;
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
