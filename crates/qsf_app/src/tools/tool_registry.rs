use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::models::ModelToolDefinition;
use crate::project_docs::ProjectDocService;
use crate::session::SessionState;

use super::calculator_tool::CalculatorTool;
use super::read_project_doc_tool::ReadProjectDocTool;
use super::search_project_docs_tool::SearchProjectDocsTool;
use super::tool_request::{ToolCategory, ToolRequest, ToolSideEffectLevel};
use super::tool_result::ToolResult;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolMetadata {
    pub name: &'static str,
    pub description: &'static str,
    pub category: ToolCategory,
    pub side_effect_level: ToolSideEffectLevel,
}

pub trait Tool {
    fn metadata(&self) -> ToolMetadata;

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult>;

    fn model_tool_definition(&self) -> Option<ModelToolDefinition> {
        None
    }
}

pub trait ToolContext {
    fn session_state(&self) -> Option<&SessionState> {
        None
    }

    fn project_doc_service(&self) -> Option<&ProjectDocService> {
        None
    }
}

#[derive(Default)]
pub struct EmptyToolContext;

impl ToolContext for EmptyToolContext {}

pub struct ToolRegistry {
    calculator: CalculatorTool,
    recall_turn: super::RecallTurnTool,
    search_project_docs: SearchProjectDocsTool,
    read_project_doc: ReadProjectDocTool,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            calculator: CalculatorTool,
            recall_turn: super::RecallTurnTool,
            search_project_docs: SearchProjectDocsTool,
            read_project_doc: ReadProjectDocTool,
        }
    }
}

impl ToolRegistry {
    pub fn metadata_for(&self, tool_name: &str) -> Option<ToolMetadata> {
        match tool_name {
            super::CALCULATOR_TOOL_NAME => Some(self.calculator.metadata()),
            super::RECALL_TURN_TOOL_NAME => Some(self.recall_turn.metadata()),
            super::SEARCH_PROJECT_DOCS_TOOL_NAME => Some(self.search_project_docs.metadata()),
            super::READ_PROJECT_DOC_TOOL_NAME => Some(self.read_project_doc.metadata()),
            _ => None,
        }
    }

    fn dispatch(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        match request.tool_name.as_str() {
            super::CALCULATOR_TOOL_NAME => self.calculator.execute(request, ctx),
            super::RECALL_TURN_TOOL_NAME => self.recall_turn.execute(request, ctx),
            super::SEARCH_PROJECT_DOCS_TOOL_NAME => self.search_project_docs.execute(request, ctx),
            super::READ_PROJECT_DOC_TOOL_NAME => self.read_project_doc.execute(request, ctx),
            _ => bail!("unknown tool `{}`", request.tool_name),
        }
    }

    pub fn validate_request(&self, request: &ToolRequest) -> Result<ToolMetadata> {
        let metadata = self
            .metadata_for(&request.tool_name)
            .ok_or_else(|| anyhow::anyhow!("unknown tool `{}`", request.tool_name))?;

        if !request
            .permission
            .allows(metadata.category, metadata.side_effect_level)
        {
            bail!(
                "tool `{}` requires category={:?} side_effect_level={:?}, but permission only allows {:?} up to {:?}",
                metadata.name,
                metadata.category,
                metadata.side_effect_level,
                request.permission.allowed_categories,
                request.permission.max_side_effect_level
            );
        }

        Ok(metadata)
    }

    pub fn validate_and_execute(
        &self,
        request: &ToolRequest,
        ctx: &dyn ToolContext,
    ) -> Result<(ToolMetadata, ToolResult)> {
        let metadata = self.validate_request(request)?;
        let result = self.dispatch(request, ctx)?;
        Ok((metadata, result))
    }

    pub fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let (_, result) = self.validate_and_execute(request, ctx)?;
        Ok(result)
    }

    pub fn model_tool_definitions_for(&self, names: &[&str]) -> Vec<ModelToolDefinition> {
        names
            .iter()
            .filter_map(|name| match *name {
                super::CALCULATOR_TOOL_NAME => self.calculator.model_tool_definition(),
                super::RECALL_TURN_TOOL_NAME => self.recall_turn.model_tool_definition(),
                super::SEARCH_PROJECT_DOCS_TOOL_NAME => {
                    self.search_project_docs.model_tool_definition()
                }
                super::READ_PROJECT_DOC_TOOL_NAME => self.read_project_doc.model_tool_definition(),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ToolRegistry;
    use crate::project_docs::ProjectDocService;
    use crate::tools::ProjectDocToolContext;
    use crate::tools::{ToolCategory, ToolPermission, ToolRequest, ToolSideEffectLevel};
    use std::path::PathBuf;

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
        let ctx = ProjectDocToolContext { service: &service };
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
        let ctx = ProjectDocToolContext { service: &service };
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
