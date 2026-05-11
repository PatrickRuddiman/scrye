//! Search-engine wrapper. Defines the [`Indexer`] trait and the stable
//! public types (`MessageId`, `Mode`, `IndexSubmit`, `Hit`, `SearchQuery`,
//! `SearchResponse`) that downstream tasks build against.
//!
//! Two backends ship:
//!   - [`WitchcraftIndexer`] — the production indexer, backed by
//!     `dropbox/witchcraft`. Default. Only pulled in on Linux targets
//!     because the upstream toolchain (candle, fbgemm-rs) is Linux-only
//!     for scryd's purposes. Activate the `witchcraft-backend` feature
//!     explicitly to be sure; it's on by default.
//!   - [`InMemoryIndexer`] — a test stub. Available unconditionally;
//!     callers that want it instead of witchcraft (unit tests, dev
//!     workflows on non-Linux hosts) can either depend on this crate
//!     `default-features = false` or instantiate `InMemoryIndexer`
//!     directly even when the witchcraft feature is on.

pub mod document;
pub mod drainer;
pub mod in_memory;
pub mod indexer;
pub mod searcher;
pub mod snippet;

// The witchcraft binding only resolves on Linux + macOS targets
// (per the target-conditional deps in Cargo.toml). On other hosts
// (Windows dev boxes) the feature flag is still on but the deps
// are absent — gate the module on the same target predicate so the
// workspace builds cleanly everywhere.
#[cfg(all(
    feature = "witchcraft-backend",
    any(target_os = "linux", target_os = "macos")
))]
pub mod witchcraft_handle;

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Stable internal identifier for a single message. Newtype around a string;
/// the storage slice's `messages.message_id` column is the source of truth.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    #[default]
    FullText,
    Semantic,
    Hybrid,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullText => "fulltext",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fulltext" => Ok(Self::FullText),
            "semantic" => Ok(Self::Semantic),
            "hybrid" => Ok(Self::Hybrid),
            other => Err(format!("unknown mode `{other}`; expected fulltext|semantic|hybrid")),
        }
    }
}

/// What the indexer task hands to the indexer per drained queue row.
#[derive(Debug, Clone)]
pub struct IndexSubmit {
    pub message_id: MessageId,
    pub document: String,
}

/// What the api slice asks the indexer for.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub q: String,
    pub mode: Mode,
    pub k: usize,
    /// Caller-driven account scope. Empty = all accounts. Non-empty
    /// = only return hits whose `account_id` is in this set. The
    /// consumer's higher-layer api populates this from whatever
    /// per-end-user policy it enforces.
    pub account_ids: Vec<String>,
}

/// Single search hit. The api slice fills in metadata and snippet from
/// `meta.sqlite.messages` after this returns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hit {
    pub message_id: MessageId,
    pub score: f32,
    /// Witchcraft's matching chunk-region for semantic-mode hits, when
    /// available. None for full-text/hybrid; the api slice derives a snippet
    /// from `body_md` in those modes.
    pub semantic_snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<Hit>,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("witchcraft binding not yet wired up; enable feature `witchcraft-backend` and complete the upstream integration")]
    BackendNotImplemented,
    #[error("upstream witchcraft error: {0}")]
    Upstream(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("witchcraft binding not yet wired up; enable feature `witchcraft-backend` and complete the upstream integration")]
    BackendNotImplemented,
    #[error("upstream witchcraft error: {0}")]
    Upstream(String),
}

pub use document::build as build_document;
pub use drainer::{Drainer, INDEXER_BATCH, MAX_ATTEMPTS};
pub use in_memory::InMemoryIndexer;
pub use indexer::Indexer;
pub use searcher::{Searcher, K_MIN, K_MULTIPLIER};
pub use snippet::render as render_snippet;

#[cfg(all(
    feature = "witchcraft-backend",
    any(target_os = "linux", target_os = "macos")
))]
pub use witchcraft_handle::WitchcraftIndexer;
