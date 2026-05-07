use std::sync::Mutex;

use async_trait::async_trait;
use scryd_imap::sink::{FetchedMessage, MessageSink, SyncStateUpdate};
use scryd_imap::{handle_uidvalidity_change, ClientError, ConnState, Connection};

#[derive(Default)]
struct StubSink {
    submitted: Mutex<Vec<FetchedMessage>>,
    tombstoned: Mutex<Vec<String>>,
    sync_state_updates: Mutex<Vec<(String, String, SyncStateUpdate)>>,
}

#[async_trait]
impl MessageSink for StubSink {
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
        self.sync_state_updates
            .lock()
            .unwrap()
            .push((account_id.to_string(), folder.to_string(), update));
        Ok(())
    }
}

#[tokio::test]
async fn handle_change_resets_watermark_and_routes_to_backfill() {
    let mut conn = Connection::new("primary", "INBOX");
    conn.state = ConnState::Polling;
    let sink = StubSink::default();

    handle_uidvalidity_change(&mut conn, &sink, 9999)
        .await
        .unwrap();

    // State machine routed back to InitialBackfilling.
    match &conn.state {
        ConnState::InitialBackfilling { last_uid, target } => {
            assert_eq!(*last_uid, 0);
            assert_eq!(*target, None);
        }
        other => panic!("expected InitialBackfilling, got {other:?}"),
    }

    // Sink received exactly one update with new uidvalidity + cleared
    // watermark.
    let updates = sink.sync_state_updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    let (acct, folder, update) = &updates[0];
    assert_eq!(acct, "primary");
    assert_eq!(folder, "INBOX");
    assert_eq!(update.uidvalidity, Some(9999));
    assert_eq!(update.last_seen_uid, Some(0));
}
