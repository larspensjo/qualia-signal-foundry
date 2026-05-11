use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

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

    fn execute(&self, request: &ToolRequest) -> Result<ToolResult>;
}

#[derive(Default)]
pub struct ToolRegistry {
    calculator: CalculatorTool,
}

impl ToolRegistry {
    pub fn metadata_for(&self, tool_name: &str) -> Option<ToolMetadata> {
        match tool_name {
            super::CALCULATOR_TOOL_NAME => Some(self.calculator.metadata()),
            _ => None,
        }
    }

    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResult> {
        match request.tool_name.as_str() {
            super::CALCULATOR_TOOL_NAME => self.calculator.execute(request),
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
    ) -> Result<(ToolMetadata, ToolResult)> {
        let metadata = self.validate_request(request)?;
        let result = self.dispatch(request)?;
        Ok((metadata, result))
    }

    pub fn execute(&self, request: &ToolRequest) -> Result<ToolResult> {
        let (_, result) = self.validate_and_execute(request)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::ToolRegistry;
    use crate::tools::{ToolCategory, ToolPermission, ToolRequest, ToolSideEffectLevel};

    #[test]
    fn registry_rejects_requests_without_matching_permission() {
        let registry = ToolRegistry::default();
        let request = ToolRequest {
            tool_name: super::super::CALCULATOR_TOOL_NAME.to_string(),
            input: "1 + 2".to_string(),
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
