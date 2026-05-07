use scryd_storage::{AccountHealth, StorageHandle, SyncStateUpdate};
use std::str::FromStr;
use tempfile::TempDir;

async fn seed_account(h: &StorageHandle, id: &str) {
    h.with_writer(|conn| {
        conn.execute(
            "INSERT INTO accounts (account_id, host, port, username, folders_json, active, mirrored_at) \
             VALUES (?1, 'imap.example.com', 993, 'u', '[\"INBOX\"]', 1, 0)",
            rusqlite::params![id],
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn merge_update_only_writes_set_fields() {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 1).unwrap();
    seed_account(&h, "primary").await;

    h.update_sync_state(
        "primary",
        "INBOX",
        SyncStateUpdate {
            uidvalidity: Some(42),
            last_seen_uid: Some(100),
            account_health: Some(AccountHealth::Active),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let row = h.get_sync_state("primary", "INBOX").await.unwrap().unwrap();
    assert_eq!(row.uidvalidity, Some(42));
    assert_eq!(row.last_seen_uid, 100);
    assert_eq!(row.account_health, AccountHealth::Active);
    assert!(row.last_idle_at.is_none());
    assert!(row.last_error.is_none());

    // Subsequent update changes only one field.
    h.update_sync_state(
        "primary",
        "INBOX",
        SyncStateUpdate {
            last_idle_at: Some(1_700_000_000),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let row2 = h.get_sync_state("primary", "INBOX").await.unwrap().unwrap();
    assert_eq!(row2.uidvalidity, Some(42), "uidvalidity preserved");
    assert_eq!(row2.last_idle_at, Some(1_700_000_000));
}

#[test]
fn account_health_round_trips_through_string() {
    for h in [
        AccountHealth::Unknown,
        AccountHealth::Active,
        AccountHealth::Degraded,
        AccountHealth::AuthRejected,
        AccountHealth::QuotaExceeded,
        AccountHealth::Unreachable,
        AccountHealth::TlsFailed,
    ] {
        let s = h.to_string();
        let parsed = AccountHealth::from_str(&s).unwrap();
        assert_eq!(parsed, h, "round-trip failed for {h:?} → {s:?}");
    }
}

#[test]
fn account_health_rejects_unknown_string() {
    assert!(AccountHealth::from_str("garbage").is_err());
}

#[tokio::test]
async fn get_sync_state_returns_none_for_unknown() {
    let dir = TempDir::new().unwrap();
    let h = StorageHandle::open(dir.path(), 1).unwrap();
    assert!(h.get_sync_state("ghost", "INBOX").await.unwrap().is_none());
}
