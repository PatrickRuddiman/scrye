//! In-memory indexer for tests. Stores `(MessageId, document_text)` pairs in
//! a Mutex; `search()` does case-insensitive substring matching across the
//! query terms and returns hits ordered by total term-occurrence count.
//!
//! Not a real search engine. Sufficient for downstream tasks (drainer,
//! search query) to exercise their wrapper code without a live witchcraft
//! integration.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::indexer::Indexer;
use crate::{
    Hit, IndexError, IndexSubmit, MessageId, SearchError, SearchQuery, SearchResponse,
};

#[derive(Default)]
pub struct InMemoryIndexer {
    docs: Mutex<Vec<(MessageId, String)>>,
}

impl InMemoryIndexer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test/inspection helper: number of indexed documents.
    pub fn len(&self) -> usize {
        self.docs.lock().expect("not poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl Indexer for InMemoryIndexer {
    async fn submit(&self, submit: IndexSubmit) -> Result<(), IndexError> {
        let mut docs = self.docs.lock().expect("not poisoned");
        // Replace existing entry for the same id (idempotent re-submit).
        docs.retain(|(id, _)| id != &submit.message_id);
        docs.push((submit.message_id, submit.document));
        Ok(())
    }

    async fn remove(&self, id: &MessageId) -> Result<(), IndexError> {
        let mut docs = self.docs.lock().expect("not poisoned");
        docs.retain(|(existing, _)| existing != id);
        Ok(())
    }

    async fn truncate(&self) -> Result<(), IndexError> {
        let mut docs = self.docs.lock().expect("not poisoned");
        docs.clear();
        Ok(())
    }

    async fn search(&self, query: &SearchQuery) -> Result<SearchResponse, SearchError> {
        let docs = self.docs.lock().expect("not poisoned");
        let terms: Vec<String> = query
            .q
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect();

        let mut scored: Vec<(MessageId, f32)> = Vec::new();
        for (id, text) in docs.iter() {
            let lc = text.to_lowercase();
            let score: f32 = if terms.is_empty() {
                1.0
            } else {
                terms.iter().map(|t| lc.matches(t).count() as f32).sum()
            };
            if score > 0.0 || terms.is_empty() {
                scored.push((id.clone(), score));
            }
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(query.k);

        let hits = scored
            .into_iter()
            .map(|(id, score)| Hit {
                message_id: id,
                score,
                semantic_snippet: None,
            })
            .collect();
        Ok(SearchResponse { hits })
    }
}
