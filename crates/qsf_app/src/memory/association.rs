//! Re-export of the persisted association from qsf_memory.
//! Kept for backwards compatibility with existing import paths.
pub use qsf_memory::{ASSOCIATION_SCHEMA_VERSION, Association, ensure_current_association_schema};
