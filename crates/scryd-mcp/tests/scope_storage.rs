//! `AccountScope::allowed_account_ids` resolves only the owned account(s),
//! matching `accounts.username` case-insensitively against `USER_EMAIL`.

mod common;

use common::*;
use scryd_mcp::AccountScope;

#[tokio::test]
async fn allowed_account_ids_returns_only_owned_case_insensitive() {
    let seed = seed().await;
    // Scope email is lowercase `owner@example.com`; the stored username is
    // mixed-case `Owner@Example.com`. Only the owned account must resolve.
    let allowed = Seed::scope()
        .allowed_account_ids(&seed.storage)
        .await
        .unwrap();
    assert_eq!(allowed, vec![OWNED_ID.to_string()]);
}

#[tokio::test]
async fn allowed_account_ids_empty_for_unowned_email() {
    let seed = seed().await;
    let scope = AccountScope::new("nobody@example.com").unwrap();
    let allowed = scope.allowed_account_ids(&seed.storage).await.unwrap();
    assert!(allowed.is_empty());
}
