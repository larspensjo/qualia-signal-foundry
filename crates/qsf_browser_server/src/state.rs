use std::sync::Arc;

use qsf_memory::{LoadedStore, StoreLoadError, load_existing};

use crate::cli::Args;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    load_result: Result<LoadedStore, StoreLoadError>,
    store_path: std::path::PathBuf,
    session_state_path: std::path::PathBuf,
}

impl AppState {
    pub fn load(args: &Args) -> Self {
        let load_result = load_existing(&args.store);
        let session_state_path = args
            .store
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("session-state.json");
        Self {
            inner: Arc::new(Inner {
                load_result,
                store_path: args.store.clone(),
                session_state_path,
            }),
        }
    }

    pub fn store_path(&self) -> &std::path::Path {
        &self.inner.store_path
    }

    pub fn session_state_path(&self) -> &std::path::Path {
        &self.inner.session_state_path
    }

    pub fn loaded(&self) -> Result<&LoadedStore, &StoreLoadError> {
        self.inner.load_result.as_ref()
    }
}
