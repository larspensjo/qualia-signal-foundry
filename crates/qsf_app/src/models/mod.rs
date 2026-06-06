mod mock_model;
mod model_client;
mod model_role;
mod openai_provider;
mod openai_tool_client;
mod tool_dispatch;

pub use mock_model::MockModelClient;
pub use model_client::{
    ModelClient, ModelMessage, ModelMessageRole, ModelRequest, ModelResponse, ModelResponseFormat,
    ModelToolCall, ModelToolDefinition, ModelUsage, invoke_model_role,
};
pub use model_role::{ModelOutputExpectation, ModelRole, ModelRoleId};
pub use openai_provider::{
    OpenAiProviderModelClient, build_client, build_client_from_env, requested_provider_from_env,
};
pub use tool_dispatch::{ProjectDocToolBudget, dispatch_model_tool_calls};
