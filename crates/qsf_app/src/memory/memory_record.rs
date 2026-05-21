//! Re-export of the persisted memory record from qsf_memory.
//! Kept for backwards compatibility with existing import paths.
pub use qsf_memory::{
    MEMORY_RECORD_SCHEMA_VERSION, MemoryRecord, MemoryRecordKind, ensure_current_memory_schema,
};
