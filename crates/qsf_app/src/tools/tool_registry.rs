use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::models::ModelToolDefinition;
use crate::project_docs::ProjectDocService;
use crate::session::SessionState;

use super::calculator_tool::CalculatorTool;
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
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            calculator: CalculatorTool,
            recall_turn: super::RecallTurnTool,
        }
    }
}

impl ToolRegistry {
    pub fn metadata_for(&self, tool_name: &str) -> Option<ToolMetadata> {
        match tool_name {
            super::CALCULATOR_TOOL_NAME => Some(self.calculator.metadata()),
            super::RECALL_TURN_TOOL_NAME => Some(self.recall_turn.metadata()),
            _ => None,
        }
    }

    fn dispatch(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        match request.tool_name.as_str() {
            super::CALCULATOR_TOOL_NAME => self.calculator.execute(request, ctx),
            super::RECALL_TURN_TOOL_NAME => self.recall_turn.execute(request, ctx),
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
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ToolRegistry;
    use crate::tools::{ToolCategory, ToolPermission, ToolRequest, ToolSideEffectLevel};

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
