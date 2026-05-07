//! Reconcile `accounts` rows from a parsed `scryd_config::Config`. The
//! credential never lands in the DB — only the structural fields do.

use rusqlite::params;
use scryd_config::{AccountCfg, Config};

use crate::accounts::fetch_account;
use crate::db::StorageError;
use crate::handle::StorageHandle;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileDiff {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub inactivated: Vec<String>,
}

impl StorageHandle {
    /// Mirror the config's account list into the DB. Inserts new accounts,
    /// updates changed structural fields on existing accounts (host, port,
    /// username, folders), and marks accounts no longer in the config
    /// `active = 0` (never deleting them — message rows survive).
    pub async fn reconcile_from_config(
        &self,
        cfg: &Config,
    ) -> Result<ReconcileDiff, StorageError> {
        let configured: Vec<ConfigSnapshot> = cfg
            .accounts
            .iter()
            .map(ConfigSnapshot::from_cfg)
            .collect();

        self.with_writer(move |conn| {
            let mut diff = ReconcileDiff::default();
            let now = epoch_now();

            // Process configured accounts: insert or update.
            for snap in &configured {
                let existing = fetch_account(conn, &snap.id)?;
                match existing {
                    None => {
                        conn.execute(
                            "INSERT INTO accounts (account_id, host, port, username, \
                                folders_json, active, mirrored_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
                            params![
                                snap.id,
                                snap.host,
                                snap.port,
                                snap.username,
                                snap.folders_json,
                                now,
                            ],
                        )?;
                        diff.added.push(snap.id.clone());
                    }
                    Some(prev) => {
                        let structural_changed = prev.host != snap.host
                            || prev.port != snap.port
                            || prev.username != snap.username
                            || serde_json::to_string(&prev.folders).unwrap_or_default()
                                != snap.folders_json
                            || !prev.active;
                        conn.execute(
                            "UPDATE accounts SET host = ?1, port = ?2, username = ?3, \
                                folders_json = ?4, active = 1, mirrored_at = ?5 \
                             WHERE account_id = ?6",
                            params![
                                snap.host,
                                snap.port,
                                snap.username,
                                snap.folders_json,
                                now,
                                snap.id,
                            ],
                        )?;
                        if structural_changed {
                            diff.updated.push(snap.id.clone());
                        }
                    }
                }
            }

            // Inactivate accounts present in DB but absent from config.
            let configured_ids: std::collections::HashSet<&str> =
                configured.iter().map(|s| s.id.as_str()).collect();
            let mut stmt =
                conn.prepare("SELECT account_id FROM accounts WHERE active = 1")?;
            let ids: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<_, _>>()?;
            drop(stmt);
            for id in ids {
                if !configured_ids.contains(id.as_str()) {
                    conn.execute(
                        "UPDATE accounts SET active = 0, mirrored_at = ?1 \
                         WHERE account_id = ?2",
                        params![now, id],
                    )?;
                    diff.inactivated.push(id);
                }
            }

            Ok(diff)
        })
        .await
    }
}

struct ConfigSnapshot {
    id: String,
    host: String,
    port: u16,
    username: String,
    folders_json: String,
}

impl ConfigSnapshot {
    fn from_cfg(a: &AccountCfg) -> Self {
        let folders = a
            .folders
            .clone()
            .unwrap_or_else(|| vec!["INBOX".to_string()]);
        let folders_json = serde_json::to_string(&folders).expect("string vec serializes");
        Self {
            id: a.id.clone(),
            host: a.host.clone(),
            port: a.port,
            username: a.user.clone(),
            folders_json,
        }
    }
}

fn epoch_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
