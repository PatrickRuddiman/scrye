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
//!     that work is split across two trait methods the drainer
//!     schedules independently (v0.3.6):
//!       - `flush_embeddings` runs per non-empty drainer tick. Bounded
//!         to `FLUSH_EMBED_BATCH` docs per call so the state Mutex
//!         releases frequently. Required for new docs to be findable
//!         (witchcraft's `search` linearly scans un-clustered chunks).
//!       - `flush_index` runs only when the drainer goes idle OR a
//!         per-doc threshold accumulates. `witchcraft::index_chunks`
//!         is already threshold-gated internally, but v0.3.5 called
//!         it after every batch and forced a cascade every time —
//!         v0.3.6 lets the buffer fill so cascades are rare and
//!         amortized.
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

/// Maximum dirty docs witchcraft embeds per `flush_embeddings` call.
/// Bounded so the state `Mutex` releases frequently enough that
/// `submit()` and `search()` can preempt long-running embed passes.
/// Picked so the worst-case mutex-hold ≈ FLUSH_EMBED_BATCH ×
/// seconds-per-doc on CPU XTR — 4 keeps it inside ~15s, comfortably
/// under the daemon-read timeout. v0.3.4 passed `None` (unbounded),
/// which hung searches for hours on a 60k-message backlog. The
/// drainer loops `flush_embeddings` per tick to bridge the gap to
/// `INDEXER_BATCH = 32` without lengthening any individual call.
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

/// Embed at most `FLUSH_EMBED_BATCH` dirty docs. Returns the
/// number that were embedded this call. Does NOT run
/// `witchcraft::index_chunks` — that's the caller's separate
/// decision (v0.3.6 split). Required for new docs to become
/// searchable; witchcraft's `search` linearly scans un-clustered
/// chunks, so embedding alone closes the spec's freshness SLA.
///
/// Keeps `state.dirty` set whenever there *might* be more docs to
/// embed: `embedded == FLUSH_EMBED_BATCH` likely means the LIMIT
/// truncated, so the next call should keep going. Only clears
/// `dirty` when this call embedded strictly fewer than the cap.
fn flush_embeddings(state: &mut State) -> Result<usize, IndexError> {
    if !state.dirty {
        return Ok(0);
    }
    let embedded = witchcraft::embed_chunks(&state.db, &state.embedder, Some(FLUSH_EMBED_BATCH))
        .map_err(|e| IndexError::Upstream(format!("embed_chunks: {e}")))?;
    if embedded < FLUSH_EMBED_BATCH {
        state.dirty = false;
    }
    Ok(embedded)
}

/// Run the witchcraft clustering cascade. Internally a no-op below
/// `L0_CAPACITY` unindexed embeddings; above the threshold can
/// cascade through L0→L3 and hold the state mutex for hundreds of
/// seconds on a multi-million-embedding store. Drainers should
/// gate this on idle or accumulated work, not call it per batch.
fn flush_index(state: &mut State) -> Result<(), IndexError> {
    witchcraft::index_chunks(&state.db, &state.device)
        .map_err(|e| IndexError::Upstream(format!("index_chunks: {e}")))?;
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

    async fn flush_embeddings(&self) -> Result<usize, IndexError> {
        let state = self.state.clone();
        let embedded = tokio::task::spawn_blocking(move || {
            let mut guard = state.lock().expect("witchcraft state mutex not poisoned");
            flush_embeddings(&mut guard)
        })
        .await
        .map_err(|e| IndexError::Upstream(format!("flush_embeddings join: {e}")))??;
        Ok(embedded)
    }

    async fn flush_index(&self) -> Result<(), IndexError> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = state.lock().expect("witchcraft state mutex not poisoned");
            flush_index(&mut guard)
        })
        .await
        .map_err(|e| IndexError::Upstream(format!("flush_index join: {e}")))??;
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
