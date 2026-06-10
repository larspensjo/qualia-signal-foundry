use std::sync::Arc;

use crate::project_docs::ProjectDocService;

use qsf_tools::ToolContext;

pub struct ProjectDocToolContext {
    pub service: Arc<ProjectDocService>,
}

impl ToolContext for ProjectDocToolContext {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::EmptyToolContext;
    use crate::tools::ToolContextAccess;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    #[test]
    fn context_exposes_service() {
        let service = ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        );
        let ctx = ProjectDocToolContext {
            service: Arc::new(service),
        };

        assert!(ctx.project_doc_service().is_some());
    }

    #[test]
    fn empty_context_returns_none() {
        assert!(EmptyToolContext.project_doc_service().is_none());
    }
}
