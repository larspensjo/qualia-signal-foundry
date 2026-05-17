use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    ComputeOnly,
    ReadOnly,
    WriteCapable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSideEffectLevel {
    None,
    ReadOnly,
    ExternalWrite,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolPermission {
    pub allowed_categories: Vec<ToolCategory>,
    pub max_side_effect_level: ToolSideEffectLevel,
}

impl ToolPermission {
    pub fn compute_only() -> Self {
        Self {
            allowed_categories: vec![ToolCategory::ComputeOnly],
            max_side_effect_level: ToolSideEffectLevel::None,
        }
    }

    pub fn allows(&self, category: ToolCategory, side_effect_level: ToolSideEffectLevel) -> bool {
        self.allowed_categories.contains(&category)
            && side_effect_level <= self.max_side_effect_level
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolRequest {
    pub tool_name: String,
    pub input: String,
    pub structured: Option<serde_json::Value>,
    pub permission: ToolPermission,
    pub requested_by: String,
}

impl ToolRequest {
    pub fn calculator(expression: impl Into<String>, requested_by: impl Into<String>) -> Self {
        Self {
            tool_name: super::CALCULATOR_TOOL_NAME.to_string(),
            input: expression.into(),
            structured: None,
            permission: ToolPermission::compute_only(),
            requested_by: requested_by.into(),
        }
    }

    pub fn recall_turn(
        call_id: impl Into<String>,
        turn_id: usize,
        requested_by: impl Into<String>,
    ) -> Self {
        let call_id = call_id.into();
        Self {
            tool_name: super::RECALL_TURN_TOOL_NAME.to_string(),
            input: format!("recall_turn call_id={call_id} turn_id={turn_id}"),
            structured: Some(serde_json::json!({
                "call_id": call_id,
                "turn_id": turn_id,
            })),
            permission: ToolPermission::compute_only(),
            requested_by: requested_by.into(),
        }
    }
}
