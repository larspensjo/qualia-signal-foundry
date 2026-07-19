//! Persisted memory record, association, retrieval, and store-loading types
//! shared between qsf_app, qsf_context, and qsf_browser_server.

pub mod association;
pub mod co_retrieval;
pub mod errors;
pub mod processed_range;
pub mod record;
pub mod retrieval;
pub mod store;

pub use association::{ASSOCIATION_SCHEMA_VERSION, Association, ensure_current_association_schema};
pub use co_retrieval::{
    CO_RETRIEVAL_INITIAL_WEIGHT, CO_RETRIEVAL_STRENGTHEN_DELTA, CROSS_TURN_ASSOCIATION_WINDOW,
    CoRetrievalDelta, CrossTurnAnchorRange, MAX_NEW_ASSOCIATIONS_PER_TURN,
    SLEEP_ASSOCIATION_INITIAL_WEIGHT, SLEEP_ASSOCIATION_STRENGTHEN_DELTA,
    generate_cross_turn_deltas, generate_cross_turn_deltas_for_anchor_range,
    generate_cross_turn_deltas_for_anchor_ranges, generate_deltas,
};
pub use errors::{SchemaVersions, ShapeError, StoreLoadError};
pub use processed_range::{ProcessedRange, ProcessedRangeKind};
pub use record::{
    MEMORY_RECORD_SCHEMA_VERSION, MemoryProvenance, MemoryRecord, MemoryRecordKind,
    MemoryTrustTier, ensure_current_memory_schema,
};
pub use retrieval::{
    AssociationPath, RetrievalResult, RetrievalScore, RetrievalStrategy, RetrievedMemory,
    SUPERSEDED_WORLD_OBSERVATION_SKIP_REASON, WORLD_OBSERVATION_DECAY_HALFLIFE_DAYS,
    retrieve_memories, retrieved_memory_ids,
};
pub use store::{
    LoadedStore, MemoryStore, MemoryStoreContents, dangling_association_ids, load_existing,
};
