//! App-specific model glue. The shared model layer (`ModelClient`, `ModelRequest`,
//! `ModelRole`, the judges, provider selection, …) lives in the `qsf_models` crate and is
//! imported directly as `qsf_models::*`; this module only holds the pieces that are genuinely
//! `qsf_app`-specific: `invoke_model_role` (records offline `RunContext` traces/events around a
//! model call) and the project-doc `tool_dispatch`.

mod model_client;
mod tool_dispatch;

pub use model_client::invoke_model_role;
pub use tool_dispatch::{ProjectDocToolBudget, dispatch_model_tool_calls};
