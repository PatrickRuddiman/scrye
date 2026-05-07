//! Credential lookup. The supervisor calls `fetch` only at LOGIN time and
//! drops the returned String immediately after `Client::login` returns.
//! The plaintext password never lives outside that scope and is never
//! interpolated into a log line (`AccountPassword`'s Debug prints
//! `[REDACTED]`).

use std::sync::Arc;

use scryd_config::Config;

/// Trait the supervisor calls at login time to obtain an account's
/// password. Implementations may read from the config file, a future
/// keyring backend, or a test fixture.
pub trait CredentialFetcher: Send + Sync {
    /// Returns the password for `account_id`, or `None` if the account is
    /// not configured.
    fn fetch(&self, account_id: &str) -> Option<String>;
}

/// Production impl: reads from the parsed `Config` held by the runtime.
pub struct ConfigCredentialFetcher {
    config: Arc<Config>,
}

impl ConfigCredentialFetcher {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }
}

impl CredentialFetcher for ConfigCredentialFetcher {
    fn fetch(&self, account_id: &str) -> Option<String> {
        self.config
            .accounts
            .iter()
            .find(|a| a.id == account_id)
            .map(|a| a.password.expose().to_string())
    }
}
