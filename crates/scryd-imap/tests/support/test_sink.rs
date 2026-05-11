//! In-memory `MessageSink` impl used by the integration tests.
//! Collects every `submit` / `tombstone` / `update_sync_state` call
//! into shared `Vec`s the test can assert on.

use std::sync::Mutex;

use async_trait::async_trait;
use scryd_imap::sink::{FetchedMessage, MessageSink, SyncStateUpdate};
use scryd_imap::ClientError;

#[derive(Default)]
pub struct CollectingSink {
    pub submitted: Mutex<Vec<FetchedMessage>>,
    pub tombstoned: Mutex<Vec<String>>,
    pub state_updates: Mutex<Vec<(String, String, SyncStateUpdate)>>,
}

impl CollectingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submitted_count(&self) -> usize {
        self.submitted.lock().unwrap().len()
    }

    pub fn tombstoned_count(&self) -> usize {
        self.tombstoned.lock().unwrap().len()
    }
}

#[async_trait]
impl MessageSink for CollectingSink {
    async fn submit(&self, fetched: FetchedMessage) -> Result<(), ClientError> {
        self.submitted.lock().unwrap().push(fetched);
        Ok(())
    }

    async fn tombstone(&self, message_id: &str) -> Result<(), ClientError> {
        self.tombstoned.lock().unwrap().push(message_id.to_string());
        Ok(())
    }

    async fn update_sync_state(
        &self,
        account_id: &str,
        folder: &str,
        update: SyncStateUpdate,
    ) -> Result<(), ClientError> {
        self.state_updates
            .lock()
            .unwrap()
            .push((account_id.to_string(), folder.to_string(), update));
        Ok(())
    }

    async fn list_local_uids(
        &self,
        account_id: &str,
        folder: &str,
        _uidvalidity: u32,
    ) -> Result<Vec<u32>, ClientError> {
        let submitted = self.submitted.lock().unwrap();
        Ok(submitted
            .iter()
            .filter(|m| m.account_id == account_id && m.folder == folder)
            .map(|m| m.server_uid)
            .collect())
    }

    async fn stored_uidvalidity(
        &self,
        account_id: &str,
        folder: &str,
    ) -> Result<Option<u32>, ClientError> {
        let updates = self.state_updates.lock().unwrap();
        Ok(updates
            .iter()
            .rev()
            .find(|(a, f, _)| a == account_id && f == folder)
            .and_then(|(_, _, u)| u.uidvalidity))
    }
}
