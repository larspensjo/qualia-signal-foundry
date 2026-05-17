use std::fmt;

use serde::{Deserialize, Serialize};

use crate::context::ContextBudget;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRoleId {
    MockResponder,
    ConversationalResponder,
    MemoryExtractor,
    SleepSummarizer,
    SessionTurnSummarizer,
    ResearchPlanner,
    Critic,
}

impl ModelRoleId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MockResponder => "mock_responder",
            Self::ConversationalResponder => "conversational_responder",
            Self::MemoryExtractor => "memory_extractor",
            Self::SleepSummarizer => "sleep_summarizer",
            Self::SessionTurnSummarizer => "session_turn_summarizer",
            Self::ResearchPlanner => "research_planner",
            Self::Critic => "critic",
        }
    }
}

impl fmt::Display for ModelRoleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelOutputExpectation {
    Text,
    JsonObject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelRole {
    pub role_id: ModelRoleId,
    pub purpose: String,
    pub allowed_tools: Vec<String>,
    pub context_budget: ContextBudget,
    pub default_model: String,
    pub output_expectation: ModelOutputExpectation,
}

impl ModelRole {
    pub fn predefined(role_id: ModelRoleId) -> Self {
        match role_id {
            ModelRoleId::MockResponder => Self {
                role_id,
                purpose: "Respond to direct prompts deterministically during framework smoke tests."
                    .to_string(),
                allowed_tools: vec![],
                context_budget: ContextBudget::new(4, 800),
                default_model: "gpt-5.4-nano".to_string(),
                output_expectation: ModelOutputExpectation::Text,
            },
            ModelRoleId::ConversationalResponder => Self {
                role_id,
                purpose:
                    "Produce short spoken replies from QSF-owned context and user input."
                        .to_string(),
                allowed_tools: vec![],
                context_budget: ContextBudget::new(4, 600),
                default_model: "gpt-5.4-nano".to_string(),
                output_expectation: ModelOutputExpectation::Text,
            },
            ModelRoleId::MemoryExtractor => Self {
                role_id,
                purpose: "Extract memory candidates from recent events into a structured review payload."
                    .to_string(),
                allowed_tools: vec![],
                context_budget: ContextBudget::new(6, 1_200),
                default_model: "gpt-5.4-nano".to_string(),
                output_expectation: ModelOutputExpectation::JsonObject,
            },
            ModelRoleId::SleepSummarizer => Self {
                role_id,
                purpose: "Summarize a session into explicit sleep-phase review fields without mutating state directly."
                    .to_string(),
                allowed_tools: vec![],
                context_budget: ContextBudget::new(8, 2_000),
                default_model: "gpt-5.4".to_string(),
                output_expectation: ModelOutputExpectation::JsonObject,
            },
            ModelRoleId::SessionTurnSummarizer => Self {
                role_id,
                purpose: "Summarize one aged-out human/assistant turn for continued in-session continuity."
                    .to_string(),
                allowed_tools: vec![],
                context_budget: ContextBudget::new(2, 500),
                default_model: "gpt-5.4-nano".to_string(),
                output_expectation: ModelOutputExpectation::Text,
            },
            ModelRoleId::ResearchPlanner => Self {
                role_id,
                purpose: "Turn open questions into scoped experiment or implementation proposals."
                    .to_string(),
                allowed_tools: vec![],
                context_budget: ContextBudget::new(8, 1_600),
                default_model: "gpt-5.4".to_string(),
                output_expectation: ModelOutputExpectation::Text,
            },
            ModelRoleId::Critic => Self {
                role_id,
                purpose: "Review plans and outputs critically with explicit reasoning and constraints."
                    .to_string(),
                allowed_tools: vec![],
                context_budget: ContextBudget::new(6, 1_400),
                default_model: "gpt-5.4".to_string(),
                output_expectation: ModelOutputExpectation::Text,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelOutputExpectation, ModelRole, ModelRoleId};

    #[test]
    fn sleep_summarizer_role_defaults_to_json_output() {
        let role = ModelRole::predefined(ModelRoleId::SleepSummarizer);

        assert_eq!(role.role_id, ModelRoleId::SleepSummarizer);
        assert_eq!(role.output_expectation, ModelOutputExpectation::JsonObject);
        assert_eq!(role.context_budget.max_fragments, 8);
        assert_eq!(role.context_budget.max_estimated_tokens, 2_000);
        assert!(role.allowed_tools.is_empty());
    }
}
