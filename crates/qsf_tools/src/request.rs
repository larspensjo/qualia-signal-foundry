use serde::{Deserialize, Serialize};

use crate::permission::ToolPermission;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolRequest {
    pub tool_name: String,
    pub input: String,
    pub structured: Option<serde_json::Value>,
    pub permission: ToolPermission,
    pub requested_by: String,
}

impl ToolRequest {
    pub fn new(
        tool_name: impl Into<String>,
        input: impl Into<String>,
        structured: Option<serde_json::Value>,
        permission: ToolPermission,
        requested_by: impl Into<String>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            input: input.into(),
            structured,
            permission,
            requested_by: requested_by.into(),
        }
    }
}
