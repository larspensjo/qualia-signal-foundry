use qsf_tools::{
    ToolContext, ToolDefinition, ToolMetadata, ToolRegistry as GenericToolRegistry, ToolRequest,
    ToolResult,
};

use crate::models::ModelToolDefinition;

use super::calculator_tool::CalculatorTool;
use super::read_project_doc_tool::ReadProjectDocTool;
use super::recall_turn_tool::RecallTurnTool;
use super::search_project_docs_tool::SearchProjectDocsTool;
pub struct ToolRegistry {
    inner: GenericToolRegistry,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut inner = GenericToolRegistry::default();
        inner.register(CalculatorTool);
        inner.register(RecallTurnTool);
        inner.register(SearchProjectDocsTool);
        inner.register(ReadProjectDocTool);
        Self { inner }
    }
}

impl ToolRegistry {
    pub fn register<T>(&mut self, tool: T)
    where
        T: super::Tool + 'static,
    {
        self.inner.register(tool);
    }

    pub fn metadata_for(&self, tool_name: &str) -> Option<ToolMetadata> {
        self.inner.metadata_for(tool_name)
    }

    pub fn definition_for(&self, tool_name: &str) -> Option<ToolDefinition> {
        self.inner.definition_for(tool_name)
    }

    pub fn definitions_for(&self, names: &[&str]) -> Vec<ToolDefinition> {
        self.inner.definitions_for(names)
    }

    pub fn model_tool_definitions_for(&self, names: &[&str]) -> Vec<ModelToolDefinition> {
        self.definitions_for(names)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn validate_request(&self, request: &ToolRequest) -> anyhow::Result<ToolMetadata> {
        self.inner.validate_request(request)
    }

    pub fn validate_and_execute(
        &self,
        request: &ToolRequest,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<(ToolMetadata, ToolResult)> {
        self.inner.validate_and_execute(request, ctx)
    }

    pub fn execute(
        &self,
        request: &ToolRequest,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        self.inner.execute(request, ctx)
    }
}

impl From<ToolDefinition> for ModelToolDefinition {
    fn from(value: ToolDefinition) -> Self {
        Self::new(value.name, value.description, value.parameters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::ProjectDocService;
    use crate::tools::{ProjectDocToolContext, ToolCategory, ToolPermission, ToolSideEffectLevel};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn fixture_service() -> ProjectDocService {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures");
        ProjectDocService::new(fixtures.clone(), fixtures.join("allowlist_basic.toml"))
    }

    #[test]
    fn calculator_exposes_model_tool_definition() {
        let registry = ToolRegistry::default();
        let definitions =
            registry.model_tool_definitions_for(&[crate::tools::CALCULATOR_TOOL_NAME]);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].name, crate::tools::CALCULATOR_TOOL_NAME);
        assert!(definitions[0].parameters.get("properties").is_some());
    }

    #[test]
    fn recall_turn_exposes_model_tool_definition() {
        let registry = ToolRegistry::default();
        let definitions =
            registry.model_tool_definitions_for(&[crate::tools::RECALL_TURN_TOOL_NAME]);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].name, crate::tools::RECALL_TURN_TOOL_NAME);
        assert!(
            definitions[0].parameters["properties"]
                .get("turn_id")
                .is_some()
        );
    }

    #[test]
    fn registry_exposes_project_doc_tools() {
        let registry = ToolRegistry::default();
        let definitions = registry.model_tool_definitions_for(&[
            crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME,
            crate::tools::READ_PROJECT_DOC_TOOL_NAME,
        ]);
        let names: Vec<&str> = definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect();

        assert_eq!(
            names,
            vec![
                crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME,
                crate::tools::READ_PROJECT_DOC_TOOL_NAME,
            ]
        );
    }

    #[test]
    fn registry_metadata_for_project_doc_tools() {
        let registry = ToolRegistry::default();

        for name in [
            crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME,
            crate::tools::READ_PROJECT_DOC_TOOL_NAME,
        ] {
            let metadata = registry.metadata_for(name).expect("metadata present");
            assert_eq!(metadata.name, name);
            assert_eq!(metadata.category, ToolCategory::ReadOnly);
            assert_eq!(metadata.side_effect_level, ToolSideEffectLevel::ReadOnly);
        }
    }

    #[test]
    fn registry_dispatches_search_project_docs() {
        let service = fixture_service();
        let ctx = ProjectDocToolContext {
            service: Arc::new(service),
        };
        let registry = ToolRegistry::default();
        let request = ToolRequest {
            tool_name: crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            input: "Maturity".to_string(),
            structured: Some(serde_json::json!({ "query": "Maturity" })),
            permission: ToolPermission::read_only(),
            requested_by: "test".to_string(),
        };

        let result = registry.execute(&request, &ctx).unwrap();

        assert_eq!(
            result.tool_name,
            crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME
        );
        assert_eq!(result.category, ToolCategory::ReadOnly);
    }

    #[test]
    fn registry_dispatches_read_project_doc() {
        let service = fixture_service();
        let ctx = ProjectDocToolContext {
            service: Arc::new(service),
        };
        let registry = ToolRegistry::default();
        const FIXTURE_DOC_PATH: &str = "sample_concept.md";
        let request = ToolRequest {
            tool_name: crate::tools::READ_PROJECT_DOC_TOOL_NAME.to_string(),
            input: FIXTURE_DOC_PATH.to_string(),
            structured: Some(serde_json::json!({ "path": FIXTURE_DOC_PATH })),
            permission: ToolPermission::read_only(),
            requested_by: "test".to_string(),
        };

        let result = registry.execute(&request, &ctx).unwrap();

        assert_eq!(result.tool_name, crate::tools::READ_PROJECT_DOC_TOOL_NAME);
        assert_eq!(result.category, ToolCategory::ReadOnly);
    }

    #[test]
    fn registry_rejects_requests_without_matching_permission() {
        let registry = ToolRegistry::default();
        let request = ToolRequest {
            tool_name: super::super::CALCULATOR_TOOL_NAME.to_string(),
            input: "1 + 2".to_string(),
            structured: None,
            permission: ToolPermission {
                allowed_categories: vec![ToolCategory::ReadOnly],
                max_side_effect_level: ToolSideEffectLevel::ReadOnly,
            },
            requested_by: "test".to_string(),
        };

        let error = registry.validate_request(&request).unwrap_err();

        assert!(error.to_string().contains("requires category"));
    }
}
