//! Tool registry, requests, results, and deterministic compute-only tools.

pub mod calculator_tool;
pub mod project_doc_tool;
pub mod read_project_doc_tool;
pub mod recall_turn_tool;
pub mod responder_tool_context;
pub mod search_project_docs_tool;
pub mod tool_registry;
pub mod tool_request;
pub mod tool_result;

pub use calculator_tool::CALCULATOR_TOOL_NAME;
pub use project_doc_tool::ProjectDocToolContext;
pub use read_project_doc_tool::{READ_PROJECT_DOC_TOOL_NAME, ReadProjectDocTool};
pub use recall_turn_tool::{RECALL_TURN_TOOL_NAME, RecallTurnTool, SessionToolContext};
pub use responder_tool_context::ResponderToolContext;
pub use search_project_docs_tool::{SEARCH_PROJECT_DOCS_TOOL_NAME, SearchProjectDocsTool};
pub use tool_registry::{EmptyToolContext, Tool, ToolContext, ToolMetadata, ToolRegistry};
pub use tool_request::{ToolCategory, ToolPermission, ToolRequest, ToolSideEffectLevel};
pub use tool_result::ToolResult;
