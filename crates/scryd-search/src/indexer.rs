//! Indexer trait the drainer (task 10) and api slice (task 18) consume.
//! Both `InMemoryIndexer` and (eventually) `WitchcraftHandle` implement it.

use async_trait::async_trait;

use crate::{IndexError, IndexSubmit, MessageId, SearchError, SearchQuery, SearchResponse};

/// Operations the search-engine wrapper exposes upstream.
///
/// Implementations:
/// - [`crate::InMemoryIndexer`]: lightweight stub; default for tests.
/// - `WitchcraftHandle` (gated on feature `witchcraft-backend`): the
///   production binding.
#[async_trait]
pub trait Indexer: Send + Sync {
    async fn submit(&self, submit: IndexSubmit) -> Result<(), IndexError>;
    async fn remove(&self, id: &MessageId) -> Result<(), IndexError>;
    async fn truncate(&self) -> Result<(), IndexError>;
    async fn search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError>;
}
