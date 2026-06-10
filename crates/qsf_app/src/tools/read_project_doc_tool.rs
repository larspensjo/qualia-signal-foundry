use anyhow::{Context, Result};
use serde_json::json;

use super::{
    Tool, ToolContext, ToolContextAccess, ToolDefinition, ToolMetadata, ToolRequest, ToolResult,
};
use qsf_tools::ToolCategory;
use qsf_tools::ToolSideEffectLevel;

pub const READ_PROJECT_DOC_TOOL_NAME: &str = "read_project_doc";

const DEFAULT_MAX_TOKENS_FOCUSED: usize = 1200;
const DEFAULT_MAX_TOKENS_NO_FOCUS: usize = 2400;
const MIN_TOKENS_SCHEMA: usize = 100;
const MAX_TOKENS_HARD_CAP: usize = 4000;

pub struct ReadProjectDocTool;

impl Tool for ReadProjectDocTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: READ_PROJECT_DOC_TOOL_NAME.to_string(),
            description: "Read a focused excerpt or bounded slice of a project document."
                .to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let service = ctx
            .project_doc_service()
            .context("read_project_doc requires ProjectDocToolContext")?;
        let args = request
            .structured
            .as_ref()
            .context("read_project_doc requires structured arguments")?;
        let path = args
            .get("path")
            .and_then(|value| value.as_str())
            .context("read_project_doc requires `path`")?;
        let focus = args
            .get("focus")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let default_budget = if focus.is_some() {
            DEFAULT_MAX_TOKENS_FOCUSED
        } else {
            DEFAULT_MAX_TOKENS_NO_FOCUS
        };
        let max_tokens = args
            .get("max_tokens")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .filter(|value| *value >= MIN_TOKENS_SCHEMA)
            .unwrap_or(default_budget)
            .min(MAX_TOKENS_HARD_CAP);

        let doc = service.read(path, focus, max_tokens)?;

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: serde_json::to_string(&doc)?,
            numeric_value: None,
            observation_summary: format!(
                "read_project_doc returned {} bytes from `{}` (is_full={}, omitted_sections={}).",
                doc.content.len(),
                doc.path,
                doc.is_full,
                doc.omitted_sections.len()
            ),
        })
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            READ_PROJECT_DOC_TOOL_NAME,
            "Read a focused excerpt or bounded slice of a project document, with kind and maturity metadata. Use after search_project_docs.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "focus": { "type": "string" },
                    "max_tokens": {
                        "type": "integer",
                        "minimum": MIN_TOKENS_SCHEMA,
                        "maximum": MAX_TOKENS_HARD_CAP
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{DocRead, ProjectDocService};
    use crate::tools::{
        EmptyToolContext, ProjectDocToolContext, Tool, ToolPermission, ToolRequest,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn service() -> ProjectDocService {
        ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        )
    }

    fn long_doc_service() -> (TempDir, ProjectDocService) {
        let temp_dir = TempDir::new().unwrap();
        let allowlist = temp_dir.path().join("allowlist.toml");
        let doc_path = temp_dir.path().join("long_doc.md");
        let body = format!(
            "# Long Doc\n\n## Body\n{}\n",
            "lorem ipsum dolor sit amet ".repeat(2000)
        );

        fs::write(&doc_path, body).unwrap();
        fs::write(
            &allowlist,
            r#"include=["long_doc.md"]
exclude=[]"#,
        )
        .unwrap();

        let service = ProjectDocService::new(temp_dir.path(), allowlist);
        (temp_dir, service)
    }

    fn make_request(path: &str, focus: Option<&str>, max_tokens: u64) -> ToolRequest {
        let mut args = serde_json::json!({ "path": path, "max_tokens": max_tokens });
        if let Some(focus) = focus {
            args["focus"] = serde_json::Value::String(focus.to_string());
        }
        ToolRequest {
            tool_name: READ_PROJECT_DOC_TOOL_NAME.to_string(),
            input: format!("read {path}"),
            structured: Some(args),
            permission: ToolPermission::read_only(),
            requested_by: "test".to_string(),
        }
    }

    fn read_output(path: &str, focus: Option<&str>, max_tokens: u64) -> String {
        let service = service();
        let ctx = ProjectDocToolContext {
            service: std::sync::Arc::new(service),
        };
        ReadProjectDocTool
            .execute(&make_request(path, focus, max_tokens), &ctx)
            .unwrap()
            .output_text
    }

    fn read_output_from_service(
        service: &ProjectDocService,
        path: &str,
        focus: Option<&str>,
        max_tokens: u64,
    ) -> String {
        let ctx = ProjectDocToolContext {
            service: std::sync::Arc::new(service.clone()),
        };
        ReadProjectDocTool
            .execute(&make_request(path, focus, max_tokens), &ctx)
            .unwrap()
            .output_text
    }

    #[test]
    fn read_returns_doc_content() {
        let doc: DocRead =
            serde_json::from_str(&read_output("sample_concept.md", None, 4000)).unwrap();
        assert!(doc.content.contains("Concept: Sample"));
    }

    #[test]
    fn read_clamps_max_tokens_to_hard_cap() {
        let (_temp_dir, service) = long_doc_service();
        let at_cap =
            read_output_from_service(&service, "long_doc.md", None, MAX_TOKENS_HARD_CAP as u64);
        let over_cap = read_output_from_service(&service, "long_doc.md", None, 10_000);
        assert_eq!(at_cap, over_cap);
    }

    #[test]
    fn read_treats_sub_minimum_max_tokens_as_default() {
        let (_temp_dir, service) = long_doc_service();
        let default = read_output_from_service(
            &service,
            "long_doc.md",
            None,
            DEFAULT_MAX_TOKENS_NO_FOCUS as u64,
        );
        let below_minimum = read_output_from_service(&service, "long_doc.md", None, 0);
        assert_eq!(default, below_minimum);
    }

    #[test]
    fn read_refuses_out_of_allowlist() {
        let service = service();
        let ctx = ProjectDocToolContext {
            service: std::sync::Arc::new(service),
        };
        let err = ReadProjectDocTool
            .execute(&make_request("outside.txt", None, 4000), &ctx)
            .unwrap_err();
        assert!(err.to_string().contains("not in allowlist"));
    }

    #[test]
    fn read_fails_without_project_doc_context() {
        let err = ReadProjectDocTool
            .execute(
                &make_request("sample_concept.md", None, 4000),
                &EmptyToolContext,
            )
            .unwrap_err();
        assert!(err.to_string().contains("ProjectDocToolContext"));
    }
}
