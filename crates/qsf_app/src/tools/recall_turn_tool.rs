use std::sync::Arc;

use anyhow::{Context, Result};

use crate::session::{SessionState, is_turn_summarized};

use super::{
    Tool, ToolContext, ToolContextAccess, ToolDefinition, ToolMetadata, ToolRequest, ToolResult,
};
use qsf_tools::ToolCategory;
use qsf_tools::ToolSideEffectLevel;

pub const RECALL_TURN_TOOL_NAME: &str = "recall_turn";

pub struct RecallTurnTool;

pub struct SessionToolContext {
    pub state: Arc<SessionState>,
}

impl ToolContext for SessionToolContext {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Tool for RecallTurnTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: RECALL_TURN_TOOL_NAME.to_string(),
            description: "Recall verbatim text for a summarized conversation turn by turn_id"
                .to_string(),
            category: ToolCategory::ComputeOnly,
            side_effect_level: ToolSideEffectLevel::None,
        }
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let state = ctx
            .session_state()
            .context("recall_turn requires SessionToolContext")?;
        let turn_id = request
            .structured
            .as_ref()
            .and_then(|value| value.get("turn_id"))
            .and_then(|value| value.as_u64())
            .context("recall_turn requires integer argument `turn_id`")?
            as usize;
        let turn = state
            .turns
            .get(turn_id)
            .with_context(|| format!("turn {turn_id} does not exist"))?;
        anyhow::ensure!(
            is_turn_summarized(state, turn_id),
            "turn {turn_id} is not summarized and cannot be recalled"
        );

        let output_text = format!(
            "[Turn {turn_id}]\n[User]\n{}\n\n[Assistant]\n{}",
            turn.user_input, turn.assistant_response
        );

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ComputeOnly,
            side_effect_level: ToolSideEffectLevel::None,
            input: request.input.clone(),
            output_text,
            numeric_value: None,
            observation_summary: format!("Recall returned verbatim text for turn {turn_id}."),
        })
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            RECALL_TURN_TOOL_NAME,
            "Recall verbatim text for a summarized conversation turn by turn_id.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "turn_id": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "The summarized turn index to recall."
                    }
                },
                "required": ["turn_id"],
                "additionalProperties": false
            }),
        ))
    }
}
