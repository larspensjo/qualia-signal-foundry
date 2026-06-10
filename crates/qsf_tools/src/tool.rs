use std::any::Any;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::definition::ToolDefinition;
use crate::permission::{ToolCategory, ToolSideEffectLevel};
use crate::request::ToolRequest;
use crate::result::ToolResult;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub side_effect_level: ToolSideEffectLevel,
}

pub trait Tool: Send + Sync {
    fn metadata(&self) -> ToolMetadata;

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult>;

    fn definition(&self) -> Option<ToolDefinition> {
        None
    }
}

pub trait ToolContext {
    fn as_any(&self) -> &dyn Any;
}

#[derive(Default)]
pub struct EmptyToolContext;

impl ToolContext for EmptyToolContext {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Vec<std::sync::Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register<T>(&mut self, tool: T)
    where
        T: Tool + 'static,
    {
        self.tools.push(std::sync::Arc::new(tool));
    }

    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(std::sync::Arc::from(tool));
    }

    pub fn metadata_for(&self, tool_name: &str) -> Option<ToolMetadata> {
        self.tools.iter().find_map(|tool| {
            let metadata = tool.metadata();
            if metadata.name == tool_name {
                Some(metadata)
            } else {
                None
            }
        })
    }

    pub fn definition_for(&self, tool_name: &str) -> Option<ToolDefinition> {
        self.tools.iter().find_map(|tool| {
            let metadata = tool.metadata();
            if metadata.name != tool_name {
                return None;
            }

            tool.definition().or_else(|| {
                Some(ToolDefinition::new(
                    metadata.name,
                    metadata.description,
                    serde_json::json!({
                        "type": "object",
                        "properties": {},
                        "additionalProperties": true
                    }),
                ))
            })
        })
    }

    pub fn definitions_for(&self, names: &[&str]) -> Vec<ToolDefinition> {
        names
            .iter()
            .filter_map(|name| self.definition_for(name))
            .collect()
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

    fn dispatch(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        self.tools
            .iter()
            .find_map(|tool| {
                let metadata = tool.metadata();
                (metadata.name == request.tool_name).then_some(tool.as_ref())
            })
            .ok_or_else(|| anyhow::anyhow!("unknown tool `{}`", request.tool_name))?
            .execute(request, ctx)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolPermission;

    struct AddTool;

    impl Tool for AddTool {
        fn metadata(&self) -> ToolMetadata {
            ToolMetadata {
                name: "add".to_string(),
                description: "Add two numbers".to_string(),
                category: ToolCategory::ComputeOnly,
                side_effect_level: ToolSideEffectLevel::None,
            }
        }

        fn execute(&self, request: &ToolRequest, _: &dyn ToolContext) -> Result<ToolResult> {
            let args = request.structured.as_ref().expect("args");
            let left = args
                .get("left")
                .and_then(serde_json::Value::as_i64)
                .unwrap();
            let right = args
                .get("right")
                .and_then(serde_json::Value::as_i64)
                .unwrap();
            Ok(ToolResult {
                tool_name: request.tool_name.clone(),
                category: ToolCategory::ComputeOnly,
                side_effect_level: ToolSideEffectLevel::None,
                input: request.input.clone(),
                output_text: (left + right).to_string(),
                numeric_value: Some((left + right) as f64),
                observation_summary: "added".to_string(),
            })
        }

        fn definition(&self) -> Option<ToolDefinition> {
            Some(ToolDefinition::new(
                "add",
                "Add two numbers",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "left": {"type": "integer"},
                        "right": {"type": "integer"}
                    },
                    "required": ["left", "right"],
                    "additionalProperties": false
                }),
            ))
        }
    }

    #[test]
    fn registry_validates_and_executes_boxed_tools() {
        let mut registry = ToolRegistry::default();
        registry.register(AddTool);

        let request = ToolRequest::new(
            "add",
            "left=2 right=3",
            Some(serde_json::json!({"left": 2, "right": 3})),
            ToolPermission::compute_only(),
            "test",
        );
        let result = registry.execute(&request, &EmptyToolContext).unwrap();

        assert_eq!(result.numeric_value, Some(5.0));
        assert_eq!(registry.definition_for("add").unwrap().name, "add");
    }
}
