use anyhow::Context;

use super::model_client::{ModelClient, ModelRequest, ModelResponse};
#[cfg(feature = "openai")]
use super::model_client::{ModelMessageRole, ModelResponseFormat, ModelUsage};

#[cfg(feature = "openai")]
mod enabled {
    use anyhow::Context;
    use openai_provider_kit::{
        ChatMessage, ChatRole, FinishReason, LlmProvider, LlmRequest, ModelId, OpenAiProvider,
        ProviderKind, ResponseFormat,
    };

    use super::{
        ModelClient, ModelMessageRole, ModelRequest, ModelResponse, ModelResponseFormat, ModelUsage,
    };

    pub struct OpenAiProviderModelClient {
        provider: OpenAiProvider,
        runtime: tokio::runtime::Runtime,
    }

    impl OpenAiProviderModelClient {
        pub fn from_env() -> anyhow::Result<Self> {
            let provider = OpenAiProvider::from_env().context(
                "failed to initialize OpenAI provider from environment; expected OPENAI_API_KEY",
            )?;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("failed to build Tokio runtime for OpenAI provider")?;

            Ok(Self { provider, runtime })
        }
    }

    impl ModelClient for OpenAiProviderModelClient {
        fn client_name(&self) -> &str {
            "openai"
        }

        fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            let provider_request = LlmRequest::new(
                ModelId::new(ProviderKind::OpenAi, request.model_name.clone()),
                request
                    .messages
                    .iter()
                    .map(|message| {
                        ChatMessage::new(map_message_role(message.role), message.content.clone())
                    })
                    .collect(),
            );
            let provider_request = match request.temperature {
                Some(temperature) => provider_request.with_temperature(temperature),
                None => provider_request,
            };
            let provider_request = match request.max_output_tokens {
                Some(max_output_tokens) => {
                    provider_request.with_max_output_tokens(max_output_tokens)
                }
                None => provider_request,
            };
            let provider_request = match request.response_format {
                ModelResponseFormat::Text => {
                    provider_request.with_response_format(ResponseFormat::Text)
                }
                ModelResponseFormat::JsonObject => provider_request.with_json_response(),
            };

            let provider_response = self
                .runtime
                .block_on(self.provider.complete(&provider_request))
                .context("OpenAI provider request failed")?;

            let usage = provider_response.usage();
            let finish_reason = match provider_response.finish_reason() {
                FinishReason::Stop => "stop",
                FinishReason::MaxTokens => "max_tokens",
                FinishReason::ContentFilter => "content_filter",
                FinishReason::Unknown => "unknown",
            };

            Ok(ModelResponse::from_text(
                request,
                self.client_name(),
                provider_response.model_id().model_name().to_string(),
                provider_response.content().to_string(),
            )
            .with_usage(
                ModelUsage::new(usage.input_tokens, usage.output_tokens)
                    .with_cached_input_tokens(usage.cached_input_tokens),
            )
            .with_finish_reason(finish_reason))
        }
    }

    fn map_message_role(role: ModelMessageRole) -> ChatRole {
        match role {
            ModelMessageRole::System => ChatRole::System,
            ModelMessageRole::User => ChatRole::User,
            ModelMessageRole::Assistant => ChatRole::Assistant,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{OpenAiProviderModelClient, map_message_role};
        use crate::models::{
            ModelClient, ModelMessage, ModelMessageRole, ModelRequest, ModelRole, ModelRoleId,
        };

        #[test]
        fn message_role_mapping_matches_provider_roles() {
            assert_eq!(
                format!("{:?}", map_message_role(ModelMessageRole::System)),
                "System"
            );
            assert_eq!(
                format!("{:?}", map_message_role(ModelMessageRole::User)),
                "User"
            );
            assert_eq!(
                format!("{:?}", map_message_role(ModelMessageRole::Assistant)),
                "Assistant"
            );
        }

        #[test]
        #[ignore = "requires OPENAI_API_KEY and the openai feature"]
        fn openai_provider_completes_trivial_request() {
            if std::env::var("OPENAI_API_KEY").is_err() {
                return;
            }

            let client = OpenAiProviderModelClient::from_env().unwrap();
            let request = ModelRequest::new(
                ModelRole::predefined(ModelRoleId::MockResponder),
                vec![
                    ModelMessage::system("Reply briefly with the word ok."),
                    ModelMessage::user("Say ok."),
                ],
            )
            .with_max_output_tokens(16);

            let response = client.complete(&request).unwrap();

            assert_eq!(response.provider_name, "openai");
            assert!(!response.output_text.trim().is_empty());
        }
    }
}

#[cfg(not(feature = "openai"))]
mod enabled {
    use anyhow::bail;

    use super::{ModelClient, ModelRequest, ModelResponse};

    pub struct OpenAiProviderModelClient;

    impl OpenAiProviderModelClient {
        pub fn from_env() -> anyhow::Result<Self> {
            bail!("qsf_app was built without the `openai` feature")
        }
    }

    impl ModelClient for OpenAiProviderModelClient {
        fn client_name(&self) -> &str {
            "openai"
        }

        fn complete(&self, _request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            bail!("qsf_app was built without the `openai` feature")
        }
    }
}

pub use enabled::OpenAiProviderModelClient;

fn requested_provider(input: Option<&str>) -> &'static str {
    match input {
        Some(value) if value.eq_ignore_ascii_case("openai") => "openai",
        _ => "mock",
    }
}

pub fn requested_provider_from_env() -> &'static str {
    requested_provider(std::env::var("QSF_MODEL_PROVIDER").ok().as_deref())
}

pub fn build_client(provider_name: &str) -> anyhow::Result<Box<dyn ModelClient>> {
    match requested_provider(Some(provider_name)) {
        "openai" => OpenAiProviderModelClient::from_env()
            .map(|client| Box::new(client) as Box<dyn ModelClient>)
            .context("failed to build OpenAI model client from environment"),
        _ => Ok(Box::new(super::mock_model::MockModelClient::default())),
    }
}

pub fn build_client_from_env() -> anyhow::Result<Box<dyn ModelClient>> {
    build_client(requested_provider_from_env())
}

#[cfg(test)]
mod tests {
    use super::{build_client, requested_provider};

    #[test]
    fn provider_defaults_to_mock_when_value_is_absent() {
        assert_eq!(requested_provider(None), "mock");
        assert_eq!(requested_provider(Some("unknown")), "mock");
        assert_eq!(requested_provider(Some("openai")), "openai");
    }

    #[test]
    fn build_client_returns_mock_for_unknown_provider() {
        let client = build_client("not-a-provider").unwrap();

        assert_eq!(client.client_name(), "mock");
    }
}
