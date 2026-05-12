use std::collections::HashMap;

use anyhow::bail;
use serde_json::json;

use super::model_client::{ModelClient, ModelRequest, ModelResponse, ModelUsage};
use super::model_role::ModelRoleId;

#[derive(Clone, Debug)]
struct MockFixture {
    output_text: String,
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Clone, Debug)]
pub struct MockModelClient {
    provider_name: String,
    fixtures: HashMap<ModelRoleId, MockFixture>,
}

impl Default for MockModelClient {
    fn default() -> Self {
        let mut fixtures = HashMap::new();
        fixtures.insert(
            ModelRoleId::MockResponder,
            MockFixture {
                output_text: "Mock responder completed the request deterministically.".to_string(),
                input_tokens: 18,
                output_tokens: 11,
            },
        );
        fixtures.insert(
            ModelRoleId::MemoryExtractor,
            MockFixture {
                output_text: json!({
                    "memory_candidates": [
                        {
                            "summary": "The system compared memory retrieval strategies.",
                            "importance": 0.82
                        }
                    ],
                    "source_excerpt": "memory retrieval remained deterministic"
                })
                .to_string(),
                input_tokens: 46,
                output_tokens: 34,
            },
        );
        fixtures.insert(
            ModelRoleId::SleepSummarizer,
            MockFixture {
                output_text: json!({
                    "session_summary": "Mock sleep summarizer reviewed the session and preserved explicit follow-up fields.",
                    "memory_candidates": [
                        {
                            "summary": "Model roles now flow through the same event and trace artifacts as other subsystems.",
                            "importance": 0.88,
                            "source_reference": "session-transcript:phase-6"
                        }
                    ],
                    "open_questions": [
                        "Should future sleep consolidation split extraction and summarization into separate roles?"
                    ],
                    "decision_candidates": [
                        "Keep model invocation traces linked to role ids, client names, and token usage."
                    ],
                    "future_context_hints": [
                        "Include the selected provider and structured output expectation in later sleep reports."
                    ],
                    "review_notes": [
                        "All extracted items remain proposals pending manual review."
                    ]
                })
                .to_string(),
                input_tokens: 64,
                output_tokens: 73,
            },
        );
        fixtures.insert(
            ModelRoleId::ResearchPlanner,
            MockFixture {
                output_text: "Mock research planner proposes a focused experiment before widening model orchestration scope."
                    .to_string(),
                input_tokens: 27,
                output_tokens: 16,
            },
        );
        fixtures.insert(
            ModelRoleId::Critic,
            MockFixture {
                output_text: "Mock critic recommends adding failure traces before introducing more provider complexity."
                    .to_string(),
                input_tokens: 31,
                output_tokens: 17,
            },
        );

        Self {
            provider_name: "mock".to_string(),
            fixtures,
        }
    }
}

impl MockModelClient {
    pub fn with_fixture(mut self, role_id: ModelRoleId, output_text: impl Into<String>) -> Self {
        self.fixtures.insert(
            role_id,
            MockFixture {
                output_text: output_text.into(),
                input_tokens: 24,
                output_tokens: 24,
            },
        );
        self
    }
}

impl ModelClient for MockModelClient {
    fn client_name(&self) -> &str {
        &self.provider_name
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        let fixture = self.fixtures.get(&request.role.role_id).ok_or_else(|| {
            anyhow::anyhow!(
                "mock model fixture missing for role `{}`",
                request.role.role_id
            )
        })?;

        if request.messages.is_empty() {
            bail!(
                "mock model request for role `{}` must include at least one message",
                request.role.role_id
            );
        }

        let usage = ModelUsage::new(fixture.input_tokens, fixture.output_tokens)
            .with_cached_input_tokens(fixture.input_tokens / 3)
            .with_estimated_cost_usd(0.0);

        Ok(ModelResponse::from_text(
            request,
            self.client_name(),
            request.model_name.clone(),
            fixture.output_text.clone(),
        )
        .with_usage(usage)
        .with_finish_reason("stop"))
    }
}

#[cfg(test)]
mod tests {
    use super::MockModelClient;
    use crate::models::{ModelClient, ModelMessage, ModelRequest, ModelRole, ModelRoleId};

    #[test]
    fn sleep_summarizer_mock_response_contains_structured_output() {
        let client = MockModelClient::default();
        let role = ModelRole::predefined(ModelRoleId::SleepSummarizer);
        let request = ModelRequest::new(role, vec![ModelMessage::user("Summarize the session")]);

        let response = client.complete(&request).unwrap();

        assert_eq!(response.provider_name, "mock");
        assert!(response.structured_output.is_some());
        assert_eq!(response.usage.unwrap().cached_input_tokens, 21);
    }
}
