//! Read-only project-document introspection: allowlist evaluation,
//! metadata extraction, lexical search, and bounded reads.

pub mod allowlist;
pub mod metadata;
pub mod read;
pub mod search;
pub mod service;
pub mod types;

pub use allowlist::Allowlist;
pub use metadata::{kind_for_path, last_reviewed_for, maturity_for};
pub use read::read;
pub use search::search;
pub use service::ProjectDocService;
pub use types::{DocHit, DocKind, DocRead, MatchStrength, MaturityTag};
