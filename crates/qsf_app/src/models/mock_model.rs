use std::collections::HashMap;

use anyhow::bail;
use serde_json::json;

use super::model_client::{
    ModelClient, ModelMessageRole, ModelRequest, ModelResponse, ModelToolCall, ModelUsage,
};
use super::model_role::ModelRoleId;

const CALCULATOR_TOOL_NAME: &str = "calculator";
const RECALL_TURN_TOOL_NAME: &str = "recall_turn";

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
            ModelRoleId::ConversationalResponder,
            MockFixture {
                output_text: "I am a QSF runtime voice loop. I turn your speech into events, assemble context, and answer through the framework before speech playback."
                    .to_string(),
                input_tokens: 58,
                output_tokens: 28,
            },
        );
        fixtures.insert(
            ModelRoleId::SleepSummarizer,
            MockFixture {
                output_text: json!({
                    "session_summary": "Mock sleep summarizer reviewed the session and preserved explicit follow-up fields.",
                    "memory_candidates": [],
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
                output_tokens: 49,
            },
        );
        fixtures.insert(
            ModelRoleId::SessionTurnSummarizer,
            MockFixture {
                output_text:
                    "The user and assistant discussed QSF session continuity in one aged-out turn."
                        .to_string(),
                input_tokens: 36,
                output_tokens: 14,
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
        fixtures.insert(
            ModelRoleId::CoherenceJudge,
            MockFixture {
                output_text: json!({ "contradictions": [] }).to_string(),
                input_tokens: 40,
                output_tokens: 8,
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

        let mut response = ModelResponse::from_text(
            request,
            self.client_name(),
            request.model_name.clone(),
            fixture.output_text.clone(),
        )
        .with_usage(usage.clone());

        if request.role.role_id == ModelRoleId::ConversationalResponder {
            if let Some(result) = calculator_result_from_tool_message(request) {
                response = ModelResponse::from_text(
                    request,
                    self.client_name(),
                    request.model_name.clone(),
                    format!("The result is {result}."),
                )
                .with_usage(usage)
                .with_finish_reason("stop");
            } else if request
                .tools
                .iter()
                .any(|tool| tool.name == RECALL_TURN_TOOL_NAME)
                && request
                    .messages
                    .iter()
                    .all(|message| message.role != ModelMessageRole::Tool)
                && request
                    .last_user_message()
                    .map(|message| message.to_ascii_lowercase().contains("recall turn"))
                    .unwrap_or(false)
            {
                response = response
                    .with_tool_calls(vec![ModelToolCall::new(
                        "mock-recall-0",
                        RECALL_TURN_TOOL_NAME,
                        json!({ "turn_id": 0 }),
                    )])
                    .with_finish_reason("tool_calls");
            } else if request
                .tools
                .iter()
                .any(|tool| tool.name == CALCULATOR_TOOL_NAME)
                && request
                    .last_user_message()
                    .and_then(extract_arithmetic_expression)
                    .is_some()
            {
                let expression = request
                    .last_user_message()
                    .and_then(extract_arithmetic_expression)
                    .expect("expression was just checked");
                response = response
                    .with_tool_calls(vec![ModelToolCall::new(
                        "mock-calculator-0",
                        CALCULATOR_TOOL_NAME,
                        json!({ "expression": expression }),
                    )])
                    .with_finish_reason("tool_calls");
            } else {
                response = response.with_finish_reason("stop");
            }
        } else {
            response = response.with_finish_reason("stop");
        }

        Ok(response)
    }
}

fn extract_arithmetic_expression(message: &str) -> Option<String> {
    let user_text = message
        .rsplit_once("[User]\n")
        .map(|(_, text)| text)
        .unwrap_or(message);
    let expression = user_text
        .chars()
        .filter(|ch| ch.is_ascii_digit() || matches!(ch, '+' | '-' | '*' | '/' | '(' | ')' | '.'))
        .collect::<String>();

    if expression
        .chars()
        .any(|ch| matches!(ch, '+' | '-' | '*' | '/'))
        && expression.chars().any(|ch| ch.is_ascii_digit())
    {
        Some(expression)
    } else {
        None
    }
}

fn calculator_result_from_tool_message(request: &ModelRequest) -> Option<String> {
    request.messages.iter().rev().find_map(|message| {
        if message.role != ModelMessageRole::Tool || !message.content.contains("[calculator]") {
            return None;
        }

        message
            .content
            .lines()
            .find_map(|line| line.strip_prefix("result: "))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
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
        assert_eq!(
            response.structured_output.unwrap()["memory_candidates"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(response.usage.unwrap().cached_input_tokens, 21);
    }
}
