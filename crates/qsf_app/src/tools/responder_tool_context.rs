use std::sync::Arc;

use crate::project_docs::ProjectDocService;
use crate::session::SessionState;

use qsf_tools::ToolContext;

pub struct ResponderToolContext {
    pub state: Arc<SessionState>,
    pub project_docs: Arc<ProjectDocService>,
}

impl ToolContext for ResponderToolContext {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionConfig, SessionState};
    use crate::tools::ToolContextAccess;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn test_config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: crate::session::MemorySourceConfig {
                source: "test".to_string(),
                file: None,
            },
        }
    }

    #[test]
    fn responder_context_answers_both_accessors() {
        let service = ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        );
        let state = SessionState::new(test_config());
        let ctx = ResponderToolContext {
            state: Arc::new(state),
            project_docs: Arc::new(service),
        };

        assert!(ctx.session_state().is_some());
        assert!(ctx.project_doc_service().is_some());
    }
}
