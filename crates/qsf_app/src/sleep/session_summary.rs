use anyhow::{Context, anyhow};

use crate::models::{
    ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRole, ModelRoleId,
    invoke_model_role,
};
use crate::runtime::run_context::RunContext;

use super::sleep_report::{SleepInputBundle, SleepReport, parse_sleep_report};

const SLEEP_SUMMARY_MAX_OUTPUT_TOKENS: u32 = 1536;
const SLEEP_PHASE_SYSTEM_PROMPT: &str = "You are the Qualia Signal Foundry sleep-phase summarizer. Return compact valid JSON only, with no markdown and no prose outside the JSON object. The object must contain these fields: session_summary, memory_candidates, open_questions, decision_candidates, future_context_hints, review_notes. You may include association_candidates with from_memory_candidate_index, to_memory_candidate_index, weight, and reason when a provisional link is clearly grounded. Association candidate indexes are 1-based: the first memory candidate is index 1, and index 0 is invalid. Keep memory_candidates to at most 4 items, open_questions to at most 3, decision_candidates to at most 2, future_context_hints to at most 3, and review_notes to at most 3. memory_candidates may be strings or objects; when an object includes importance, importance must be a numeric value from 0.0 to 1.0, never labels like low, medium, or high. Keep decision_candidates and association_candidates explicitly provisional and do not invent accepted decisions.";

#[derive(Clone, Debug, PartialEq)]
pub struct SleepSummaryResult {
    pub report: SleepReport,
    pub response: ModelResponse,
}

pub fn summarize_session(
    context: &mut RunContext,
    client: &dyn ModelClient,
    input: &SleepInputBundle,
) -> anyhow::Result<SleepSummaryResult> {
    let role = ModelRole::predefined(ModelRoleId::SleepSummarizer);
    let request = ModelRequest::new(
        role,
        vec![
            ModelMessage::system(SLEEP_PHASE_SYSTEM_PROMPT),
            ModelMessage::user(build_sleep_user_prompt(input)),
        ],
    )
    .with_temperature(0.0)
    .with_max_output_tokens(SLEEP_SUMMARY_MAX_OUTPUT_TOKENS);

    let response = invoke_model_role(context, client, &request)?;
    let structured_output = response.structured_output.as_ref().ok_or_else(|| {
        anyhow!(
            "sleep summarizer did not return a JSON object; finish_reason={:?}; parse_error={:?}",
            response.finish_reason,
            response.structured_output_parse_error
        )
    })?;
    let report = parse_sleep_report(structured_output).with_context(|| {
        format!(
            "failed to parse sleep report from provider `{}` model `{}`",
            response.provider_name, response.model_name
        )
    })?;

    Ok(SleepSummaryResult { report, response })
}

fn build_sleep_user_prompt(input: &SleepInputBundle) -> String {
    let mut prompt = String::new();
    prompt.push_str(&format!(
        "Source kind: {}\nSource label: {}\n\nSession text:\n{}\n",
        input.source_kind, input.source_label, input.session_text
    ));

    if !input.review_notes.is_empty() {
        prompt.push_str("\nReview notes:\n");
        for note in &input.review_notes {
            prompt.push_str("- ");
            prompt.push_str(note);
            prompt.push('\n');
        }
    }

    prompt.push_str(
        "\nReturn concise reviewable JSON output. Memory candidates may be strings or objects with summary, numeric importance from 0.0 to 1.0, and source_reference. Association candidates are optional and must include a specific reason.",
    );
    prompt
}

#[cfg(test)]
mod tests {
    use super::{SLEEP_SUMMARY_MAX_OUTPUT_TOKENS, summarize_session};
    use crate::models::{MockModelClient, ModelClient, ModelRequest, ModelResponse};
    use crate::runtime::run_context::RunContext;
    use crate::sleep::SleepInputBundle;

    #[test]
    fn summarize_session_uses_sleep_role_and_parses_report() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-sleep-core-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "sleep-core-test").unwrap();
        let input = SleepInputBundle::new(
            "session_transcript",
            "phase-7-test",
            "We kept reducers pure and want a minimal sleep summary.",
        );

        let result = summarize_session(&mut context, &MockModelClient::default(), &input).unwrap();

        assert!(
            result
                .report
                .session_summary
                .contains("Mock sleep summarizer")
        );
        assert_eq!(result.report.memory_candidates.len(), 1);
        assert_eq!(context.event_count(), 2);
        assert_eq!(context.trace_count(), 1);

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn summarize_session_requests_enough_json_budget_and_numeric_importance() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-sleep-budget-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "sleep-budget-test").unwrap();
        let input = SleepInputBundle::new(
            "session_transcript",
            "budget-test",
            "We need a compact sleep summary with numeric importance.",
        );

        summarize_session(&mut context, &AssertingSleepClient, &input).unwrap();

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    struct AssertingSleepClient;

    impl ModelClient for AssertingSleepClient {
        fn client_name(&self) -> &str {
            "asserting-sleep-client"
        }

        fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            assert_eq!(
                request.max_output_tokens,
                Some(SLEEP_SUMMARY_MAX_OUTPUT_TOKENS)
            );
            assert!(request.messages.iter().any(|message| {
                message.content.contains("numeric value from 0.0 to 1.0")
                    && message.content.contains("never labels")
            }));

            Ok(ModelResponse::from_text(
                request,
                self.client_name(),
                request.model_name.clone(),
                r#"{
                    "session_summary": "Summary.",
                    "memory_candidates": [],
                    "open_questions": [],
                    "decision_candidates": [],
                    "future_context_hints": [],
                    "review_notes": []
                }"#,
            ))
        }
    }
}
