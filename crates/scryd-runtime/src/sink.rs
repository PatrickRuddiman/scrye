//! `StorageMessageSink` — bridges scryd-imap's [`MessageSink`] trait to the
//! scryd-storage + scryd-mime pipeline. Raw bytes from a fetch land here,
//! get parsed, and write through as a `messages` row + attachments + raw
//! `.eml` file + `index_queue` row, then a notify wakes the drainer.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use scryd_imap::sink::{FetchedMessage, MessageSink, SyncStateUpdate};
use scryd_imap::ClientError;
use scryd_log::{category, log_failure};
use scryd_storage::{
    Address as StorageAddress, AccountHealth, MessageInsert, StorageHandle, SyncStateUpdate as StorageSyncStateUpdate,
};
use std::str::FromStr;
use tokio::sync::Notify;

pub struct StorageMessageSink {
    storage: StorageHandle,
    drainer_notify: Arc<Notify>,
    data_dir: PathBuf,
}

impl StorageMessageSink {
    pub fn new(storage: StorageHandle, drainer_notify: Arc<Notify>, data_dir: PathBuf) -> Self {
        Self {
            storage,
            drainer_notify,
            data_dir,
        }
    }
}

#[async_trait]
impl MessageSink for StorageMessageSink {
    async fn submit(&self, fetched: FetchedMessage) -> Result<(), ClientError> {
        let ctx = scryd_mime::ParseContext {
            account_id: &fetched.account_id,
            folder: &fetched.folder,
            server_uid: fetched.server_uid,
            internal_date: fetched.internal_date,
        };
        let outcome = scryd_mime::parse(&fetched.raw_bytes, ctx);

        let parsed = match outcome {
            scryd_mime::ParseOutcome::Parsed(m) => m,
            scryd_mime::ParseOutcome::ParsedDegraded(m, faults) => {
                for fault in faults {
                    log_failure!(
                        severity = warn,
                        category = category::SINGLE_MESSAGE_PARSE_FAILURE,
                        account_id = %fetched.account_id,
                        folder = %fetched.folder,
                        server_uid = fetched.server_uid,
                        subtype = format_args!("{fault:?}")
                    );
                }
                m
            }
            scryd_mime::ParseOutcome::Unparseable(faults) => {
                for fault in faults {
                    log_failure!(
                        severity = warn,
                        category = category::SINGLE_MESSAGE_PARSE_FAILURE,
                        account_id = %fetched.account_id,
                        folder = %fetched.folder,
                        server_uid = fetched.server_uid,
                        subtype = format_args!("{fault:?}")
                    );
                }
                // Storage-side placeholder write happens via the path that
                // produces a synthetic message_id. v1: skip-on-unparseable
                // and bubble back to the imap layer; the row never enters
                // the searchable corpus.
                return Ok(());
            }
        };

        // Synthesize the scryd-internal message id.
        let canonical = parsed.header_message_id.clone().unwrap_or_else(|| {
            format!(
                "synth-{}",
                hex_sha256(&format!(
                    "{}|{}|{}",
                    fetched.folder, fetched.uidvalidity, fetched.server_uid
                ))
            )
        });
        let message_id = format!("{}:{}", fetched.account_id, canonical);

        // Atomic raw-eml write.
        let raw_path = scryd_storage::raw::write_raw(
            &self.data_dir,
            &fetched.account_id,
            &message_id,
            parsed.date_unix,
            &fetched.raw_bytes,
        )
        .await
        .map_err(map_storage_err)?;

        // Map mime addresses to storage addresses.
        let to_storage = |mime: scryd_mime::Address| StorageAddress {
            addr: mime.addr,
            name: mime.name,
        };
        let recipients_to: Vec<StorageAddress> = parsed.to.into_iter().map(to_storage).collect();
        let recipients_cc: Vec<StorageAddress> = parsed.cc.into_iter().map(to_storage).collect();

        let attachments = parsed.attachments;
        let insert = MessageInsert {
            message_id: message_id.clone(),
            account_id: fetched.account_id.clone(),
            folder: fetched.folder.clone(),
            server_uid: fetched.server_uid,
            uidvalidity: fetched.uidvalidity,
            header_message_id: parsed.header_message_id,
            in_reply_to: parsed.in_reply_to,
            references: parsed.references,
            sender_addr: parsed.from.addr,
            sender_name: parsed.from.name,
            recipients_to,
            recipients_cc,
            subject: parsed.subject,
            date_unix: parsed.date_unix,
            raw_path: raw_path.to_string_lossy().into_owned(),
            body_md: parsed.body_md,
            size_bytes: fetched.raw_bytes.len() as u64,
        };

        self.storage
            .insert_message(insert)
            .await
            .map_err(map_storage_err)?;

        // Attachments: best-effort; storage may have a helper later, for v1
        // we skip writes (the messages row is the search-relevant unit).
        let _ = attachments;

        self.storage
            .enqueue(&message_id)
            .await
            .map_err(map_storage_err)?;
        self.drainer_notify.notify_one();

        Ok(())
    }

    async fn tombstone(&self, message_id: &str) -> Result<(), ClientError> {
        self.storage
            .tombstone(message_id)
            .await
            .map_err(map_storage_err)
    }

    async fn update_sync_state(
        &self,
        account_id: &str,
        folder: &str,
        update: SyncStateUpdate,
    ) -> Result<(), ClientError> {
        let mapped = StorageSyncStateUpdate {
            uidvalidity: update.uidvalidity,
            last_seen_uid: update.last_seen_uid,
            last_full_sync_at: update.last_full_sync_at,
            last_idle_at: update.last_idle_at,
            last_error: update.last_error,
            account_health: update
                .account_health
                .as_deref()
                .and_then(|s| AccountHealth::from_str(s).ok()),
            backoff_until: update.backoff_until,
        };
        self.storage
            .update_sync_state(account_id, folder, mapped)
            .await
            .map_err(map_storage_err)
    }

    async fn list_local_uids(
        &self,
        account_id: &str,
        folder: &str,
        uidvalidity: u32,
    ) -> Result<Vec<u32>, ClientError> {
        self.storage
            .list_uids_for(account_id, folder, uidvalidity)
            .await
            .map_err(map_storage_err)
    }

    async fn stored_uidvalidity(
        &self,
        account_id: &str,
        folder: &str,
    ) -> Result<Option<u32>, ClientError> {
        let row = self
            .storage
            .get_sync_state(account_id, folder)
            .await
            .map_err(map_storage_err)?;
        Ok(row.and_then(|r| r.uidvalidity))
    }
}

fn map_storage_err(err: scryd_storage::StorageError) -> ClientError {
    ClientError::Server(err.to_string())
}

fn hex_sha256(s: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(s.as_bytes()))
}
