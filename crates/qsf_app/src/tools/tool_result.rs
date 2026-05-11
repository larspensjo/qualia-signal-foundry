use crate::context::{ContextFragment, ContextSourceKind};

use super::tool_request::{ToolCategory, ToolSideEffectLevel};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolResult {
    pub tool_name: String,
    pub category: ToolCategory,
    pub side_effect_level: ToolSideEffectLevel,
    pub input: String,
    pub output_text: String,
    pub numeric_value: Option<f64>,
    pub observation_summary: String,
}

impl ToolResult {
    pub fn to_context_fragment(&self, score: f64) -> ContextFragment {
        let summary = self.observation_summary.clone();

        ContextFragment {
            fragment_id: format!("tool:{}", self.tool_name),
            source_kind: ContextSourceKind::ToolObservation,
            summary: summary.clone(),
            tags: vec![
                "tool".to_string(),
                self.tool_name.clone(),
                format!("category:{}", category_label(self.category)),
            ],
            score,
            estimated_tokens: estimate_tokens(&summary),
            source_reference: format!("tool:{}", self.tool_name),
            selection_reason: format!(
                "tool observation with side_effect_level={} from input `{}`",
                side_effect_label(self.side_effect_level),
                self.input
            ),
        }
    }
}

fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4).max(8)
}

fn category_label(category: ToolCategory) -> &'static str {
    match category {
        ToolCategory::ComputeOnly => "compute_only",
        ToolCategory::ReadOnly => "read_only",
        ToolCategory::WriteCapable => "write_capable",
    }
}

fn side_effect_label(level: ToolSideEffectLevel) -> &'static str {
    match level {
        ToolSideEffectLevel::None => "none",
        ToolSideEffectLevel::ReadOnly => "read_only",
        ToolSideEffectLevel::ExternalWrite => "external_write",
    }
}
