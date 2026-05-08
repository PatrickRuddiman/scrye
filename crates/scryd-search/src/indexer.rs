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
}
