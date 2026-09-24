use anyhow::Context;
use qsf_memory::{
    MemoryStore, RetrievalRequest, RetrievalResult, RetrievalStrategy, retrieve_memories,
};
use time::OffsetDateTime;

use crate::state::AppState;

pub fn load_session_memory_store(
    state: &AppState,
    qsf_session_id: &str,
) -> anyhow::Result<MemoryStore> {
    #[cfg(test)]
    {
        let mut counts = STORE_LOAD_COUNTS.lock().expect("store load counts");
        *counts
            .entry(state.continuity_memory_store_path(qsf_session_id))
            .or_default() += 1;
    }
    MemoryStore::load_or_empty(state.continuity_memory_store_path(qsf_session_id))
}

#[cfg(test)]
static STORE_LOAD_COUNTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, usize>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[cfg(test)]
pub(super) fn session_store_load_count(state: &AppState, qsf_session_id: &str) -> usize {
    STORE_LOAD_COUNTS
        .lock()
        .expect("store load counts")
        .get(&state.continuity_memory_store_path(qsf_session_id))
        .copied()
        .unwrap_or_default()
}

pub fn retrieve_session_memories(
    state: &AppState,
    qsf_session_id: &str,
    query: &str,
    strategy: RetrievalStrategy,
    limit: usize,
    evaluation_time: OffsetDateTime,
) -> anyhow::Result<RetrievalResult> {
    let store = load_session_memory_store(state, qsf_session_id)?;
    retrieve_session_memories_from_store(&store, query, strategy, limit, evaluation_time)
}

pub fn retrieve_session_memories_from_store(
    store: &MemoryStore,
    query: &str,
    strategy: RetrievalStrategy,
    limit: usize,
    evaluation_time: OffsetDateTime,
) -> anyhow::Result<RetrievalResult> {
    let request = RetrievalRequest::new(
        &store.contents().records,
        &store.contents().associations,
        query,
        strategy,
        limit,
        evaluation_time,
    );
    retrieve_memories(&request)
}

pub async fn load_session_memory_store_off_executor(
    state: AppState,
    qsf_session_id: String,
) -> anyhow::Result<MemoryStore> {
    let session_for_error = qsf_session_id.clone();
    load_once_on_blocking_executor(move || load_session_memory_store(&state, &qsf_session_id))
        .await
        .with_context(|| {
            format!("failed to load memory store off executor for session `{session_for_error}`")
        })
}

async fn load_once_on_blocking_executor<T, F>(load: F) -> anyhow::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(load)
        .await
        .context("blocking session memory store load task failed")?
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::thread::ThreadId;

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
            OffsetDateTime::UNIX_EPOCH,
        )
        .expect("result");

        assert!(result.selected.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_store_load_runs_once_on_a_blocking_thread() {
        let tempdir = TempDir::new().expect("tempdir");
        let path = tempdir.path().join("empty-store.json");
        let async_thread = std::thread::current().id();
        let load_count = Arc::new(AtomicUsize::new(0));
        let load_thread = Arc::new(std::sync::Mutex::new(None::<ThreadId>));
        let count_for_load = Arc::clone(&load_count);
        let thread_for_load = Arc::clone(&load_thread);

        let store = load_once_on_blocking_executor(move || {
            count_for_load.fetch_add(1, Ordering::SeqCst);
            *thread_for_load.lock().expect("load thread") = Some(std::thread::current().id());
            MemoryStore::load_or_empty(path)
        })
        .await
        .expect("store load");

        assert!(store.contents().records.is_empty());
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
        let blocking_thread = load_thread.lock().expect("load thread").unwrap();
        assert_ne!(blocking_thread, async_thread);
    }
}
