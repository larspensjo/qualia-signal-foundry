use qsf_memory::{MemoryStore, RetrievalResult, RetrievalStrategy, retrieve_memories};

use crate::state::AppState;

pub fn load_session_memory_store(
    state: &AppState,
    qsf_session_id: &str,
) -> anyhow::Result<MemoryStore> {
    MemoryStore::load_or_empty(state.continuity_memory_store_path(qsf_session_id))
}

pub fn retrieve_session_memories(
    state: &AppState,
    qsf_session_id: &str,
    query: &str,
    strategy: RetrievalStrategy,
    limit: usize,
) -> anyhow::Result<RetrievalResult> {
    let store = load_session_memory_store(state, qsf_session_id)?;
    retrieve_memories(
        &store.contents().records,
        &store.contents().associations,
        query,
        strategy,
        limit,
    )
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use qsf_memory::{MemoryRecord, MemoryRecordKind};
    use time::OffsetDateTime;

    fn state(tempdir: &TempDir) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            "wss://example.invalid/realtime",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state")
    }

    fn memory_record(id: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Concept,
            format!("Title {id}"),
            format!("Summary {id}"),
            vec!["test"],
            OffsetDateTime::UNIX_EPOCH,
            0.5,
            0,
            "tests",
            16,
        )
    }

    #[test]
    fn absent_memory_store_loads_as_empty() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);

        let store = load_session_memory_store(&state, "session-absent").expect("store");

        assert!(store.contents().records.is_empty());
        assert!(store.contents().associations.is_empty());
    }

    #[test]
    fn existing_memory_store_loads_records_and_associations() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let path = state.continuity_memory_store_path("session-existing");
        let mut store = MemoryStore::load_or_empty(&path).expect("store");
        store.contents_mut().records.push(memory_record("memory-1"));
        store.persist().expect("persist");

        let loaded = load_session_memory_store(&state, "session-existing").expect("loaded");

        assert_eq!(loaded.contents().records.len(), 1);
        assert_eq!(loaded.contents().records[0].id, "memory-1");
    }

    #[test]
    fn malformed_memory_store_surfaces_error() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let path = state.continuity_memory_store_path("session-malformed");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, "{not-json").expect("write");

        let error = load_session_memory_store(&state, "session-malformed").unwrap_err();

        assert!(error.to_string().contains("failed to parse memory store"));
    }

    #[test]
    fn empty_memory_store_still_supported_by_retriever() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);

        let result = retrieve_session_memories(
            &state,
            "session-empty",
            "memory query",
            RetrievalStrategy::AssociationWeighted,
            4,
        )
        .expect("result");

        assert!(result.selected.is_empty());
    }
}
