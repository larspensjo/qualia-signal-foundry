#![deny(missing_docs)]
//! Read-only ingestion and lexical retrieval for an external article corpus.
//!
//! The crate deliberately has no dependency on application, memory, or volition code. It
//! parses a producer-owned corpus marker, defensively accepts article frontmatter, persists a
//! content-hash ledger, and exposes a deterministic in-memory lexical index.

mod article;
mod config;
mod index;
mod ingest;
mod marker;
mod untrusted;

pub use article::{Article, ArticleParseError, content_hash, parse_article};
pub use config::{
    CorpusPathResolution, CorpusPathSource, WORLD_CORPUS_PATH_ENV_VAR, bundled_fixture_corpus_path,
    resolve_corpus_path,
};
pub use index::{CorpusIndex, CorpusQueryResult, QueryCandidate, tokenize};
pub use ingest::{
    CorpusIngestIssue, CorpusIngestReport, CorpusLedger, CorpusRefresh, INDEX_LEDGER_VERSION,
    load_ledger, refresh_corpus, write_ledger,
};
pub use marker::{
    CORPUS_SUPPORTED_SCHEMA_VERSION, CorpusMarker, CorpusMarkerError, CorpusSchemaDrift,
    read_marker,
};
pub use untrusted::{
    UNTRUSTED_EXTERNAL_BLOCK_END, UNTRUSTED_EXTERNAL_BLOCK_START, frame_untrusted_external,
};
