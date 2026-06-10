use anyhow::{Context, Result};
use serde_json::json;

use super::{
    Tool, ToolContext, ToolContextAccess, ToolDefinition, ToolMetadata, ToolRequest, ToolResult,
};
use qsf_tools::ToolCategory;
use qsf_tools::ToolSideEffectLevel;

pub const SEARCH_PROJECT_DOCS_TOOL_NAME: &str = "search_project_docs";

const DEFAULT_MAX_RESULTS: usize = 6;

pub struct SearchProjectDocsTool;

impl Tool for SearchProjectDocsTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            description: "Search project documentation for material related to a query."
                .to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let service = ctx
            .project_doc_service()
            .context("search_project_docs requires ProjectDocToolContext")?;
        let args = request
            .structured
            .as_ref()
            .context("search_project_docs requires structured arguments")?;
        let query = args
            .get("query")
            .and_then(|value| value.as_str())
            .context("search_project_docs requires `query`")?;
        let max_results = args
            .get("max_results")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .filter(|value| *value >= 1)
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .min(DEFAULT_MAX_RESULTS);

        let hits = service.search(query, max_results)?;

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: serde_json::to_string(&hits)?,
            numeric_value: None,
            observation_summary: format!(
                "search_project_docs returned {} hits for query `{query}`.",
                hits.len()
            ),
        })
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            SEARCH_PROJECT_DOCS_TOOL_NAME,
            "Search project documentation. Returns ranked hits with kind and maturity metadata; follow up with read_project_doc to read a focused excerpt.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 6 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{DocHit, ProjectDocService};
    use crate::tools::{
        EmptyToolContext, ProjectDocToolContext, Tool, ToolPermission, ToolRequest,
    };
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn service() -> ProjectDocService {
        ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        )
    }

    fn make_request_with_max(query: &str, max_results: u64) -> ToolRequest {
        ToolRequest {
            tool_name: SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            input: query.to_string(),
            structured: Some(serde_json::json!({ "query": query, "max_results": max_results })),
            permission: ToolPermission::read_only(),
            requested_by: "test".to_string(),
        }
    }

    fn make_request(query: &str) -> ToolRequest {
        make_request_with_max(query, 6)
    }

    #[test]
    fn search_returns_hits_with_metadata() {
        let service = service();
        let ctx = ProjectDocToolContext {
            service: std::sync::Arc::new(service),
        };
        let result = SearchProjectDocsTool
            .execute(&make_request("Maturity"), &ctx)
            .unwrap();

        assert_eq!(result.category, ToolCategory::ReadOnly);
        assert!(result.observation_summary.contains("hits"));

        let hits: Vec<DocHit> = serde_json::from_str(&result.output_text).unwrap();
        assert!(!hits.is_empty());
    }

    #[test]
    fn search_treats_zero_max_results_as_default() {
        let service = service();
        let ctx = ProjectDocToolContext {
            service: std::sync::Arc::new(service),
        };
        let result = SearchProjectDocsTool
            .execute(&make_request_with_max("Maturity", 0), &ctx)
            .unwrap();
        let hits: Vec<DocHit> = serde_json::from_str(&result.output_text).unwrap();
        assert!(!hits.is_empty());
    }

    #[test]
    fn search_fails_without_project_doc_context() {
        let err = SearchProjectDocsTool
            .execute(&make_request("anything"), &EmptyToolContext)
            .unwrap_err();
        assert!(err.to_string().contains("ProjectDocToolContext"));
    }
}
