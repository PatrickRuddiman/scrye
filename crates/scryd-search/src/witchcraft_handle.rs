//! Production binding to dropbox/witchcraft. Gated behind the
//! `witchcraft-backend` feature flag so the default workspace build
//! stays cheap.
//!
//! Structure:
//!   - One [`witchcraft::DB`] (rusqlite Connection inside) plus one
//!     [`witchcraft::Embedder`] live behind a single [`std::sync::Mutex`].
//!     witchcraft's API is sync; we hop into `tokio::task::spawn_blocking`
//!     for every Indexer trait call.
//!   - `submit` / `remove` / `truncate` only mutate the DB and flip a
//!     `dirty` flag. They do NOT run `embed_chunks` / `index_chunks` —
//!     that work is done by the drainer's `flush_pending` call after
//!     each non-empty batch, keeping the expensive embed pass in
//!     producer cadence so `search` is a pure read that returns
//!     whatever's currently in the index.
//!   - `MessageId` → witchcraft's required `Uuid` via `Uuid::new_v5` over
//!     a fixed namespace + the id bytes. Deterministic, stable across
//!     restarts.
//!   - witchcraft's `metadata` JSON column carries `{"id":"<message_id>"}`
//!     so the search path can map results back to scryd's stable id.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use witchcraft::{DB, Embedder};

use crate::indexer::Indexer;
use crate::{
    Hit, IndexError, IndexSubmit, MessageId, Mode, SearchError, SearchQuery, SearchResponse,
};

/// Fixed namespace for `MessageId` → `Uuid` v5 derivation. Generated
/// once and pinned here so re-opening an existing witchcraft db on a
/// new scryd build still finds the same row for the same message_id.
const SCRYD_NAMESPACE: Uuid = Uuid::from_u128(0xc092702c_5d38_4e2a_91d1_f16f8aa531b8);

/// Maximum dirty docs witchcraft embeds per `flush_pending` call.
/// Bounded so the state `Mutex` releases frequently enough that
/// `submit()` and `search()` can preempt long-running embed passes.
/// Picked so the worst-case mutex-hold ≈ FLUSH_EMBED_BATCH ×
/// seconds-per-doc on CPU XTR — 4 keeps it inside ~15s, comfortably
/// under the daemon-read timeout. v0.3.4 passed `None` (unbounded),
/// which hung searches for hours on a 60k-message backlog.
pub const FLUSH_EMBED_BATCH: usize = 4;

fn message_id_to_uuid(id: &MessageId) -> Uuid {
    Uuid::new_v5(&SCRYD_NAMESPACE, id.as_str().as_bytes())
}

struct State {
    db: DB,
    embedder: Embedder,
    device: candle_core::Device,
    cache: witchcraft::EmbeddingsCache,
    dirty: bool,
}

/// Production semantic + full-text indexer backed by dropbox/witchcraft.
///
/// Construct via [`WitchcraftIndexer::open`]. Implements [`Indexer`] so it
/// drops into the existing drainer / api wiring without code changes.
pub struct WitchcraftIndexer {
    state: Arc<Mutex<State>>,
}

impl WitchcraftIndexer {
    /// Open (or create) the witchcraft database at `db_path` and load the
    /// T5 GGUF weights from `assets_path`. Both paths must already exist
    /// at mode 0700 — installer + daemon-startup self-checks own that.
    pub async fn open(db_path: &Path, assets_path: &Path) -> Result<Self, IndexError> {
        let db_path = db_path.to_path_buf();
        let assets_path = assets_path.to_path_buf();
        let state = tokio::task::spawn_blocking(move || open_blocking(db_path, assets_path))
            .await
            .map_err(|e| IndexError::Upstream(format!("open join error: {e}")))??;
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }
}

fn open_blocking(db_path: PathBuf, assets_path: PathBuf) -> Result<State, IndexError> {
    let device = witchcraft::make_device();
    let embedder = Embedder::new(&device, &assets_path)
        .map_err(|e| IndexError::Upstream(format!("embedder load: {e}")))?;
    let db = DB::new(db_path).map_err(|e| IndexError::Upstream(format!("db open: {e}")))?;
    let cache = witchcraft::EmbeddingsCache::new(64);
    Ok(State {
        db,
        embedder,
        device,
        cache,
        dirty: false,
    })
}

fn flush_pending(state: &mut State) -> Result<(), IndexError> {
    if !state.dirty {
        return Ok(());
    }
    let embedded = witchcraft::embed_chunks(&state.db, &state.embedder, Some(FLUSH_EMBED_BATCH))
        .map_err(|e| IndexError::Upstream(format!("embed_chunks: {e}")))?;
    if embedded > 0 {
        witchcraft::index_chunks(&state.db, &state.device)
            .map_err(|e| IndexError::Upstream(format!("index_chunks: {e}")))?;
    }
    state.dirty = false;
    Ok(())
}

#[async_trait]
impl Indexer for WitchcraftIndexer {
    async fn submit(&self, submit: IndexSubmit) -> Result<(), IndexError> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = state.lock().expect("witchcraft state mutex not poisoned");
            let uuid = message_id_to_uuid(&submit.message_id);
            let metadata = json!({ "id": submit.message_id.as_str() }).to_string();
            guard
                .db
                .add_doc(&uuid, None, &metadata, &submit.document, None)
                .map_err(|e| IndexError::Upstream(format!("add_doc: {e}")))?;
            guard.dirty = true;
            Ok::<(), IndexError>(())
        })
        .await
        .map_err(|e| IndexError::Upstream(format!("submit join: {e}")))??;
        Ok(())
    }

    async fn remove(&self, id: &MessageId) -> Result<(), IndexError> {
        let state = self.state.clone();
        let id = id.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = state.lock().expect("witchcraft state mutex not poisoned");
            let uuid = message_id_to_uuid(&id);
            guard
                .db
                .remove_doc(&uuid)
                .map_err(|e| IndexError::Upstream(format!("remove_doc: {e}")))?;
            guard.dirty = true;
            Ok::<(), IndexError>(())
        })
        .await
        .map_err(|e| IndexError::Upstream(format!("remove join: {e}")))??;
        Ok(())
    }

    async fn truncate(&self) -> Result<(), IndexError> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = state.lock().expect("witchcraft state mutex not poisoned");
            guard.db.clear();
            guard.dirty = false;
            Ok::<(), IndexError>(())
        })
        .await
        .map_err(|e| IndexError::Upstream(format!("truncate join: {e}")))??;
        Ok(())
    }

    async fn flush_pending(&self) -> Result<(), IndexError> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = state.lock().expect("witchcraft state mutex not poisoned");
            flush_pending(&mut guard)
        })
        .await
        .map_err(|e| IndexError::Upstream(format!("flush join: {e}")))??;
        Ok(())
    }

    async fn search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError> {
        let state = self.state.clone();
        let q = query.q.clone();
        let mode = query.mode;
        let k = query.k;
        let response = tokio::task::spawn_blocking(move || {
            let mut guard = state.lock().expect("witchcraft state mutex not poisoned");

            let State {
                ref db,
                ref embedder,
                ref mut cache,
                ..
            } = &mut *guard;

            let use_fulltext = matches!(mode, Mode::FullText | Mode::Hybrid);
            let raw = witchcraft::search(
                db,
                embedder,
                cache,
                &q,
                /* threshold */ 0.0,
                k,
                use_fulltext,
                /* sql_filter */ None,
            )
            .map_err(|e| SearchError::Upstream(format!("witchcraft::search: {e}")))?;

            let hits = raw
                .into_iter()
                .filter_map(|(score, metadata, bodies, sub_idx, _date)| {
                    let id = parse_metadata_id(&metadata)?;
                    let semantic_snippet = if matches!(mode, Mode::Semantic | Mode::Hybrid) {
                        bodies.get(sub_idx as usize).cloned()
                    } else {
                        None
                    };
                    Some(Hit {
                        message_id: MessageId::new(id),
                        score,
                        semantic_snippet,
                    })
                })
                .collect();
            Ok::<SearchResponse, SearchError>(SearchResponse { hits })
        })
        .await
        .map_err(|e| SearchError::Upstream(format!("search join: {e}")))??;
        Ok(response)
    }
}

fn parse_metadata_id(metadata: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(metadata).ok()?;
    v.get("id")?.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_id_to_uuid_is_deterministic() {
        let a = message_id_to_uuid(&MessageId::new("abc"));
        let b = message_id_to_uuid(&MessageId::new("abc"));
        assert_eq!(a, b);
    }

    #[test]
    fn message_id_to_uuid_distinguishes_distinct_ids() {
        let a = message_id_to_uuid(&MessageId::new("alice"));
        let b = message_id_to_uuid(&MessageId::new("bob"));
        assert_ne!(a, b);
    }

    #[test]
    fn parse_metadata_id_extracts_id_field() {
        let m = r#"{"id":"hello@example.com"}"#;
        assert_eq!(parse_metadata_id(m).as_deref(), Some("hello@example.com"));
    }

    #[test]
    fn parse_metadata_id_none_on_malformed() {
        assert_eq!(parse_metadata_id("not json").as_deref(), None);
        assert_eq!(parse_metadata_id(r#"{"other":"x"}"#).as_deref(), None);
    }

    #[test]
    fn flush_embed_batch_stays_bounded() {
        // v0.3.4 shipped with `None` (unbounded), which held the
        // witchcraft Mutex across an entire dirty-pool flush — hung
        // searches for hours on a 60k-message backlog. The bound
        // protects against accidental regression to either `None`
        // (caught by the call site) or a too-large limit.
        assert!(FLUSH_EMBED_BATCH > 0, "must embed something per call");
        assert!(
            FLUSH_EMBED_BATCH <= 32,
            "must stay small enough that one flush completes inside the daemon-read timeout"
        );
    }
}
