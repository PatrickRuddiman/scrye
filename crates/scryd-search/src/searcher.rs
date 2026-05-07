//! Search entry point. Wraps an [`Indexer`] handle so the api slice can
//! submit a single typed query and get back ranked hits.

use std::sync::Arc;

use crate::indexer::Indexer;
use crate::{SearchError, SearchQuery, SearchResponse};

/// Floor for the K value the api slice asks for. Even when the caller's
/// limit is small, we ask the indexer for at least this many candidates so
/// post-retrieval filtering still has hits to choose from.
pub const K_MIN: usize = 200;

/// Multiplier applied to the caller's `limit` to compute K. The api slice
/// computes `K = max(limit * K_MULTIPLIER, K_MIN)` before invoking
/// [`Searcher::search`]; this constant is exported so tests can verify the
/// math.
pub const K_MULTIPLIER: usize = 5;

pub struct Searcher {
    indexer: Arc<dyn Indexer>,
}

impl Searcher {
    pub fn new(indexer: Arc<dyn Indexer>) -> Self {
        Self { indexer }
    }

    /// Run a query against the underlying indexer. The caller is responsible
    /// for setting `query.k` to `max(limit * K_MULTIPLIER, K_MIN)`; this
    /// function does not adjust it.
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError> {
        self.indexer.search(query).await
    }
}
