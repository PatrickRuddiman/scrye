use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer};

/// IMAP app password. Wraps [`SecretString`] so the bytes are zeroed on drop
/// and cannot be printed via `Display`.
pub struct AccountPassword(SecretString);

impl AccountPassword {
    pub fn new(s: String) -> Self {
        Self(SecretString::new(s.into_boxed_str()))
    }

    /// Reveal the secret. Audit every call site.
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl std::fmt::Debug for AccountPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccountPassword([REDACTED])")
    }
}

impl<'de> Deserialize<'de> for AccountPassword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::new(raw))
    }
}
