//! `USER_EMAIL` account-ownership scope.
//!
//! [`AccountScope`] resolves the mandatory `USER_EMAIL` env var to the set of
//! owned `account_id`s — the accounts whose IMAP login (`accounts.username`)
//! equals `USER_EMAIL`, matched case-insensitively. It is resolved *per tool
//! call* (a fast indexed read of active accounts) so newly added/removed
//! accounts are reflected without a daemon restart.
//!
//! The guard helpers ([`is_allowed`], [`narrow`], [`retain_allowed`]) are the
//! single chokepoint every tool funnels through. An empty allowed-set (no
//! configured account matches `USER_EMAIL`) means every tool returns
//! empty/`not_found` — never an error that would leak "you have no such
//! account".

use scryd_storage::StorageHandle;

use crate::error::ConfigError;

/// The resolved scoping credential: a single lowercased email address. The
/// owned `account_id`s are looked up fresh on each tool call via
/// [`AccountScope::allowed_account_ids`].
#[derive(Clone, Debug)]
pub struct AccountScope {
    email: String,
}

impl AccountScope {
    /// Build a scope from an email string, erroring [`ConfigError::UserEmailMissing`]
    /// when the trimmed value is empty. The email is stored lowercased so all
    /// matching is case-insensitive.
    pub fn new(email: &str) -> Result<Self, ConfigError> {
        let trimmed = email.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::UserEmailMissing);
        }
        Ok(Self {
            email: trimmed.to_lowercase(),
        })
    }

    /// Build a scope from the mandatory `USER_EMAIL` env var. Fails fast if the
    /// var is unset or empty so the daemon never serves unscoped.
    pub fn from_env() -> Result<Self, ConfigError> {
        let raw = std::env::var("USER_EMAIL").unwrap_or_default();
        Self::new(&raw)
    }

    /// The lowercased email this scope is bound to.
    pub fn email(&self) -> &str {
        &self.email
    }

    /// Resolve the owned `account_id`s: active accounts whose `username`
    /// equals this scope's email, case-insensitively. Re-read on every call so
    /// account add/remove is reflected without restart.
    pub async fn allowed_account_ids(
        &self,
        storage: &StorageHandle,
    ) -> anyhow::Result<Vec<String>> {
        let rows = storage.list_active_accounts().await?;
        Ok(rows
            .into_iter()
            .filter(|a| a.username.eq_ignore_ascii_case(&self.email))
            .map(|a| a.account_id)
            .collect())
    }
}

/// True when `account_id` is in the allowed set.
pub fn is_allowed(allowed: &[String], account_id: &str) -> bool {
    allowed.iter().any(|a| a == account_id)
}

/// Intersect a caller-supplied account list with the allowed set (narrow,
/// never widen). An empty `caller` means "no caller filter", so all `allowed`
/// ids are returned. Callers must treat the returned vec as the authoritative,
/// never-empty-means-all filter: when `allowed` itself is empty the result is
/// empty and the caller must short-circuit to an empty/`not_found` response.
pub fn narrow(allowed: &[String], caller: &[String]) -> Vec<String> {
    if caller.is_empty() {
        return allowed.to_vec();
    }
    allowed
        .iter()
        .filter(|a| caller.iter().any(|c| c == *a))
        .cloned()
        .collect()
}

/// Retain only rows whose `key(&row)` is in the allowed set.
pub fn retain_allowed<T>(
    allowed: &[String],
    rows: Vec<T>,
    key: impl Fn(&T) -> &str,
) -> Vec<T> {
    rows.into_iter()
        .filter(|r| is_allowed(allowed, key(r)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty() {
        assert!(matches!(
            AccountScope::new(""),
            Err(ConfigError::UserEmailMissing)
        ));
        assert!(matches!(
            AccountScope::new("   "),
            Err(ConfigError::UserEmailMissing)
        ));
    }

    #[test]
    fn new_accepts_value_and_lowercases() {
        let scope = AccountScope::new("  Alice@Example.COM ").unwrap();
        assert_eq!(scope.email(), "alice@example.com");
    }

    #[test]
    fn is_allowed_matches_membership() {
        let allowed = vec!["a".to_string(), "b".to_string()];
        assert!(is_allowed(&allowed, "a"));
        assert!(is_allowed(&allowed, "b"));
        assert!(!is_allowed(&allowed, "c"));
    }

    #[test]
    fn narrow_empty_caller_returns_all_allowed() {
        let allowed = vec!["a".to_string(), "b".to_string()];
        assert_eq!(narrow(&allowed, &[]), allowed);
    }

    #[test]
    fn narrow_drops_foreign_caller_ids() {
        let allowed = vec!["a".to_string(), "b".to_string()];
        // Caller asks for one owned + one foreign id; only the owned survives.
        let caller = vec!["a".to_string(), "foreign".to_string()];
        assert_eq!(narrow(&allowed, &caller), vec!["a".to_string()]);
    }

    #[test]
    fn narrow_caller_only_foreign_yields_empty() {
        let allowed = vec!["a".to_string()];
        let caller = vec!["foreign".to_string()];
        assert!(narrow(&allowed, &caller).is_empty());
    }

    #[test]
    fn retain_allowed_filters_by_key() {
        let allowed = vec!["a".to_string()];
        let rows = vec![("a", 1), ("b", 2)];
        let kept = retain_allowed(&allowed, rows, |r| r.0);
        assert_eq!(kept, vec![("a", 1)]);
    }
}
