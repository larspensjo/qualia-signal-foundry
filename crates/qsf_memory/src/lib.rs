//! Persisted memory record, association, and store-loading types
//! shared between qsf_app and qsf_browser_server.

pub mod association;
pub mod errors;
pub mod record;
pub mod store;

pub use association::{ASSOCIATION_SCHEMA_VERSION, Association, ensure_current_association_schema};
pub use errors::{SchemaVersions, ShapeError, StoreLoadError};
pub use record::{
    MEMORY_RECORD_SCHEMA_VERSION, MemoryRecord, MemoryRecordKind, ensure_current_memory_schema,
};
pub use store::{
    LoadedStore, MemoryStore, MemoryStoreContents, dangling_association_ids, load_existing,
};
