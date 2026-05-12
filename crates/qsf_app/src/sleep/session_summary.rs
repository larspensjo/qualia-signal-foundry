use anyhow::{Context, anyhow};

use crate::models::{
    ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRole, ModelRoleId,
    invoke_model_role,
};
use crate::runtime::run_context::RunContext;

use super::sleep_report::{SleepInputBundle, SleepReport, parse_sleep_report};

const SLEEP_PHASE_SYSTEM_PROMPT: &str = "You are the Qualia Signal Foundry sleep-phase summarizer. Return a single JSON object with these fields: session_summary, memory_candidates, open_questions, decision_candidates, future_context_hints, review_notes. Keep decision_candidates explicitly provisional and do not invent accepted decisions.";

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
    .with_max_output_tokens(512);

    let response = invoke_model_role(context, client, &request)?;
    let structured_output = response
        .structured_output
        .as_ref()
        .ok_or_else(|| anyhow!("sleep summarizer did not return a JSON object"))?;
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
        "\nReturn concise reviewable sleep output. Memory candidates may be strings or objects with summary, importance, and source_reference.",
    );
    prompt
}

#[cfg(test)]
mod tests {
    use super::summarize_session;
    use crate::models::MockModelClient;
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
}
