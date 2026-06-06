use crate::project_docs::ProjectDocService;
use crate::session::SessionState;

use super::tool_registry::ToolContext;

pub struct ResponderToolContext<'a> {
    pub state: &'a SessionState,
    pub project_docs: &'a ProjectDocService,
}

impl ToolContext for ResponderToolContext<'_> {
    fn session_state(&self) -> Option<&SessionState> {
        Some(self.state)
    }

    fn project_doc_service(&self) -> Option<&ProjectDocService> {
        Some(self.project_docs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionConfig, SessionState};
    use crate::tools::tool_registry::ToolContext;
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
            state: &state,
            project_docs: &service,
        };

        assert!(ctx.session_state().is_some());
        assert!(ctx.project_doc_service().is_some());
    }
}
