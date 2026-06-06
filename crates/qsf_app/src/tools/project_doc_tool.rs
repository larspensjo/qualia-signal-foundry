use crate::project_docs::ProjectDocService;

use super::tool_registry::ToolContext;

pub struct ProjectDocToolContext<'a> {
    pub service: &'a ProjectDocService,
}

impl ToolContext for ProjectDocToolContext<'_> {
    fn project_doc_service(&self) -> Option<&ProjectDocService> {
        Some(self.service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::EmptyToolContext;
    use crate::tools::tool_registry::ToolContext;
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
        let ctx = ProjectDocToolContext { service: &service };

        assert!(ctx.project_doc_service().is_some());
    }

    #[test]
    fn empty_context_returns_none() {
        assert!(EmptyToolContext.project_doc_service().is_none());
    }
}
