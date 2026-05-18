//! Indexer trait the drainer (task 10) and api slice (task 18) consume.
//! Both `InMemoryIndexer` and `WitchcraftIndexer` implement it.

use async_trait::async_trait;

use crate::{IndexError, IndexSubmit, MessageId, SearchError, SearchQuery, SearchResponse};

/// Operations the search-engine wrapper exposes upstream.
///
/// Implementations:
/// - [`crate::WitchcraftIndexer`] (default; gated on the
///   `witchcraft-backend` feature, which ships on by default):
///   the production binding to `dropbox/witchcraft`.
/// - [`crate::InMemoryIndexer`]: a lightweight stub used by tests and by
///   non-Linux dev workflows that don't want to pull the upstream
///   toolchain.
#[async_trait]
pub trait Indexer: Send + Sync {
    async fn submit(&self, submit: IndexSubmit) -> Result<(), IndexError>;
    async fn remove(&self, id: &MessageId) -> Result<(), IndexError>;
    async fn truncate(&self) -> Result<(), IndexError>;
    async fn search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError>;

    /// Flush deferred indexing work (embedding + clustering) in one
    /// call. Composition of [`Indexer::flush_embeddings`] followed by
    /// [`Indexer::flush_index`]. Default no-op for indexers that do
    /// work synchronously in `submit`.
    ///
    /// Production drainers should prefer calling `flush_embeddings`
    /// and `flush_index` separately so the cheap embed pass can run
    /// in producer cadence while the expensive clustering pass is
    /// gated to idle / threshold (v0.3.6 split).
    async fn flush_pending(&self) -> Result<(), IndexError> {
        let _ = self.flush_embeddings().await?;
        self.flush_index().await
    }

    /// Embed any dirty docs (the small per-tick work). Returns the
    /// number of docs that were embedded this call. Default no-op
    /// returns 0 so non-witchcraft indexers don't need to implement.
    ///
    /// The witchcraft backend bounds this to `FLUSH_EMBED_BATCH`
    /// docs per call so the state Mutex releases frequently enough
    /// that `submit` and `search` can interleave.
    async fn flush_embeddings(&self) -> Result<usize, IndexError> {
        Ok(0)
    }

    /// Run the clustering / index-build pass. Default no-op for
    /// indexers without a separate index pass.
    ///
    /// The witchcraft backend calls `witchcraft::index_chunks`,
    /// which is internally threshold-gated (no-op below
    /// L0_CAPACITY=1024 unindexed embeddings) but can cascade
    /// deeply when above the threshold. Drainers should only call
    /// this on idle or when accumulated work warrants the cost.
    async fn flush_index(&self) -> Result<(), IndexError> {
        Ok(())
    }
}
