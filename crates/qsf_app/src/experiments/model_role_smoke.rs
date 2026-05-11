use std::fs;

use anyhow::Context;
use serde_json::json;

use crate::models::{
    ModelMessage, ModelRequest, ModelRole, ModelRoleId, build_client, invoke_model_role,
    requested_provider_from_env,
};
use crate::observability::event_log::EventType;
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const SESSION_PROMPT: &str = "Summarize this short Phase 6 session into explicit JSON fields.";
const SESSION_TRANSCRIPT: &str = "We added a model role abstraction, kept reducers pure, and want traces to show provider, role, latency, and token usage without logging secrets.";

pub struct ModelRoleSmokeExperiment;

impl Experiment for ModelRoleSmokeExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::ModelRoleSmokeTest
    }

    fn description(&self) -> &'static str {
        "Invoke a model role through a deterministic mock client or the optional OpenAI adapter"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        self.run_with_provider(context, requested_provider_from_env())
    }
}

impl ModelRoleSmokeExperiment {
    fn run_with_provider(
        &self,
        context: &mut RunContext,
        requested_provider: &str,
    ) -> anyhow::Result<ExperimentOutcome> {
        let role = ModelRole::predefined(ModelRoleId::SleepSummarizer);
        let request = ModelRequest::new(
            role.clone(),
            vec![
                ModelMessage::system(SESSION_PROMPT),
                ModelMessage::user(SESSION_TRANSCRIPT),
            ],
        )
        .with_temperature(0.0)
        .with_max_output_tokens(256);

        context.record_event(
            EventType::InputReceived,
            json!({
                "phase": "6",
                "requested_provider": requested_provider,
                "role": &role,
                "message_count": request.messages.len(),
            }),
            None,
        )?;

        let client = build_client(requested_provider)?;
        let response = invoke_model_role(context, client.as_ref(), &request)?;

        write_model_invocation_report(context, requested_provider, &request, &response)?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "provider_name": &response.provider_name,
                "model_name": &response.model_name,
                "message": response.output_summary(),
            }),
            None,
        )?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Phase 6 model role MVP invoked `{}` through the `{}` client, recorded model-role events and traces, and preserved structured output and usage metadata for later sleep or memory experiments.",
                request.role.role_id,
                response.provider_name
            ),
            observations: vec![
                "Model roles now declare purpose, default model, allowed tools, output expectation, and context budget in one place.".to_string(),
                "Model invocations now emit explicit requested/completed or failed events plus a linked trace with request, response, latency, and token usage.".to_string(),
                "The OpenAI-backed client remains a thin adapter over `openai_provider_kit`, while the default smoke path stays deterministic through the mock client.".to_string(),
            ],
            failure_modes: vec![
                "The OpenAI-backed path is compiled only when the `openai` Cargo feature is enabled.".to_string(),
                "No cost estimation is emitted yet for OpenAI-backed calls because the provider kit does not currently expose pricing metadata.".to_string(),
            ],
            follow_up_questions: vec![
                "Should sleep summarization and memory extraction stay separate roles once Phase 7 lands?".to_string(),
                "Should model traces eventually capture bounded prompt excerpts instead of the full message list?".to_string(),
            ],
            decision_candidates: vec![
                "Keep model-provider wiring behind a synchronous `ModelClient` boundary until the runtime loop itself needs async effects.".to_string(),
                "Select the OpenAI-backed model path explicitly through environment/config rather than auto-switching when `OPENAI_API_KEY` happens to be set.".to_string(),
            ],
            extra_artifacts: vec!["model-invocation.md".to_string()],
        })
    }
}

fn write_model_invocation_report(
    context: &RunContext,
    requested_provider: &str,
    request: &ModelRequest,
    response: &crate::models::ModelResponse,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Phase 6 Model Invocation\n\n");
    markdown.push_str(&format!("- Role: `{}`\n", request.role.role_id));
    markdown.push_str(&format!("- Requested provider: `{}`\n", requested_provider));
    markdown.push_str(&format!(
        "- Resolved provider: `{}`\n",
        response.provider_name
    ));
    markdown.push_str(&format!("- Model: `{}`\n", response.model_name));
    markdown.push_str(&format!(
        "- Output summary: {}\n",
        response.output_summary()
    ));
    if let Some(usage) = &response.usage {
        markdown.push_str(&format!(
            "- Usage: input={} output={} cached_input={}\n",
            usage.input_tokens, usage.output_tokens, usage.cached_input_tokens
        ));
    }
    if let Some(structured_output) = &response.structured_output {
        markdown.push_str("\n## Structured Output\n\n```json\n");
        markdown.push_str(&serde_json::to_string_pretty(structured_output)?);
        markdown.push_str("\n```\n");
    } else {
        markdown.push_str("\n## Raw Output\n\n");
        markdown.push_str(&response.output_text);
        markdown.push('\n');
    }

    fs::write(context.run_dir().join("model-invocation.md"), markdown).with_context(|| {
        format!(
            "failed to write model invocation artifact for run {}",
            context.run_id()
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::ModelRoleSmokeExperiment;
    use crate::runtime::run_context::RunContext;

    #[test]
    fn model_smoke_experiment_writes_model_artifact() {
        let base_dir = std::env::temp_dir().join(format!("qsf-phase6-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "phase-six-test").unwrap();
        let experiment = ModelRoleSmokeExperiment;

        let outcome = experiment.run_with_provider(&mut context, "mock").unwrap();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();

        assert_eq!(context.event_count(), 4);
        assert_eq!(context.trace_count(), 1);
        assert!(outcome.summary.contains("Phase 6"));
        assert!(events.contains("ModelRoleRequested"));
        assert!(events.contains("ModelRoleCompleted"));
        assert!(
            fs::read_to_string(context.run_dir().join("model-invocation.md"))
                .unwrap()
                .contains("Structured Output")
        );

        fs::remove_dir_all(base_dir).unwrap();
    }
}
