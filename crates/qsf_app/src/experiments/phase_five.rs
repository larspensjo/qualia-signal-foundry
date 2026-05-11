use std::fs;
use std::time::Instant;

use anyhow::Context;
use serde_json::json;

use crate::context::{ContextBudget, ContextSelection, assemble_context};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::tools::{ToolMetadata, ToolRegistry, ToolRequest, ToolResult};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const TOOL_QUERY: &str = "Use the calculator tool as a deterministic observation source.";
const CALCULATOR_EXPRESSION: &str = "((3 + 5) * 2 - 4) / 3";

pub struct ToolAsPerceptionCalculatorExperiment;

impl Experiment for ToolAsPerceptionCalculatorExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::ToolAsPerceptionCalculator
    }

    fn description(&self) -> &'static str {
        "Execute a compute-only calculator tool and treat the result as a context candidate"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let registry = ToolRegistry::default();
        let request = ToolRequest::calculator(CALCULATOR_EXPRESSION, self.id());

        context.record_event(
            EventType::InputReceived,
            json!({
                "query": TOOL_QUERY,
                "expression": request.input,
            }),
            None,
        )?;

        context.record_event(
            EventType::ToolRequested,
            json!({
                "tool_name": request.tool_name,
                "input": request.input,
                "permission": request.permission,
                "requested_by": request.requested_by,
            }),
            None,
        )?;

        let (metadata, result, tool_elapsed_ns) =
            execute_tool_request(context, &registry, &request)?;
        let tool_trace = TraceRecord::new(
            context.experiment_id(),
            "tool-invocation",
            format!("tool={} input={}", request.tool_name, request.input),
            result.observation_summary.clone(),
        )
        .with_details(json!({
            "request": request,
            "metadata": metadata,
            "result": result,
        }))
        .with_latency_ns(tool_elapsed_ns);
        let tool_trace_id = tool_trace.trace_id;
        context.record_trace(tool_trace)?;
        context.record_event(
            EventType::ToolCompleted,
            json!({
                "tool_name": result.tool_name,
                "output_text": result.output_text,
                "numeric_value": result.numeric_value,
                "category": result.category,
                "side_effect_level": result.side_effect_level,
                "latency_ns": tool_elapsed_ns,
                "latency_ms": tool_elapsed_ns / 1_000_000,
            }),
            Some(tool_trace_id),
        )?;

        let candidate_fragment = result.to_context_fragment(95.0);
        let budget = ContextBudget::new(1, 48);

        context.record_event(
            EventType::ContextAssemblyRequested,
            json!({
                "budget": budget,
                "candidate_fragments": [&candidate_fragment.fragment_id],
            }),
            None,
        )?;

        let assembly_started_at = Instant::now();
        let assembly = assemble_context(vec![candidate_fragment], budget);
        let assembly_elapsed_ns = assembly_started_at
            .elapsed()
            .as_nanos()
            .try_into()
            .unwrap_or(u64::MAX);
        let context_trace = TraceRecord::new(
            context.experiment_id(),
            "context-assembly",
            "tool observation candidate fragment",
            format!(
                "selected={} omitted={} used_estimated_tokens={}",
                assembly.selected.len(),
                assembly.omitted.len(),
                assembly.used_estimated_tokens
            ),
        )
        .with_details(json!({
            "budget": budget,
            "selected": assembly.selected,
            "omitted": assembly.omitted,
        }))
        .with_latency_ns(assembly_elapsed_ns);
        let context_trace_id = context_trace.trace_id;
        context.record_trace(context_trace)?;
        context.record_event(
            EventType::ContextAssembled,
            json!({
                "selected": assembly.selected.iter().map(|selection| &selection.fragment.fragment_id).collect::<Vec<_>>(),
                "omitted": assembly.omitted.iter().map(|omission| &omission.fragment.fragment_id).collect::<Vec<_>>(),
                "used_estimated_tokens": assembly.used_estimated_tokens,
                "latency_ns": assembly_elapsed_ns,
                "latency_ms": assembly_elapsed_ns / 1_000_000,
            }),
            Some(context_trace_id),
        )?;

        write_tool_report(context, &request, &result, &assembly.selected)?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "message": result.observation_summary,
                "selected_context_fragments": assembly.selected.iter().map(|selection| &selection.fragment.fragment_id).collect::<Vec<_>>(),
            }),
            Some(context_trace_id),
        )?;

        Ok(ExperimentOutcome {
            summary: "Phase 5 tool-as-perception MVP ran a compute-only calculator tool, recorded the request and trace, and turned the result into a candidate context fragment under the existing context budget flow.".to_string(),
            observations: vec![
                "Tool execution now carries explicit category and side-effect metadata before dispatch.".to_string(),
                "The calculator result becomes a tool observation fragment before it can enter context selection.".to_string(),
                "Tool invocation traces capture request, permission, metadata, result, and latency in the same run artifact model as other subsystems.".to_string(),
            ],
            failure_modes: vec![
                "The first calculator parser supports arithmetic expressions only; it is intentionally narrow and deterministic.".to_string(),
                "The registry is still static and code-defined rather than dynamically configured.".to_string(),
            ],
            follow_up_questions: vec![
                "Should future tool observations carry confidence or freshness scores distinct from memory retrieval scores?".to_string(),
                "Should read-only external tools reuse the same permission shape or introduce a stricter approval layer?".to_string(),
            ],
            decision_candidates: vec![
                "Tool results enter context only as explicit tool-observation fragments, not implicit state mutations.".to_string(),
                "Tool requests should always declare category and maximum allowed side-effect level before execution.".to_string(),
            ],
            extra_artifacts: vec!["tool-observation.md".to_string()],
        })
    }
}

fn execute_tool_request(
    context: &mut RunContext,
    registry: &ToolRegistry,
    request: &ToolRequest,
) -> anyhow::Result<(ToolMetadata, ToolResult, u64)> {
    let started_at = Instant::now();

    match registry.validate_and_execute(request) {
        Ok((metadata, result)) => {
            let elapsed_ns = started_at
                .elapsed()
                .as_nanos()
                .try_into()
                .unwrap_or(u64::MAX);
            Ok((metadata, result, elapsed_ns))
        }
        Err(error) => {
            let elapsed_ns = started_at
                .elapsed()
                .as_nanos()
                .try_into()
                .unwrap_or(u64::MAX);
            let error_message = error.to_string();

            context
                .record_event(
                    EventType::ToolFailed,
                    json!({
                        "tool_name": request.tool_name,
                        "input": request.input,
                        "requested_by": request.requested_by,
                        "error": error_message,
                        "latency_ns": elapsed_ns,
                        "latency_ms": elapsed_ns / 1_000_000,
                    }),
                    None,
                )
                .with_context(|| {
                    format!(
                        "failed to record tool failure event for run {}",
                        context.run_id()
                    )
                })?;

            Err(error)
        }
    }
}

fn write_tool_report(
    context: &RunContext,
    request: &ToolRequest,
    result: &ToolResult,
    selected: &[ContextSelection],
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Phase 5 Tool Observation\n\n");
    markdown.push_str(&format!("- Tool: `{}`\n", request.tool_name));
    markdown.push_str(&format!("- Expression: `{}`\n", request.input));
    markdown.push_str(&format!("- Observation: {}\n", result.observation_summary));
    markdown.push_str(&format!(
        "- Selected context fragments: {}\n",
        selected
            .iter()
            .map(|selection| selection.fragment.fragment_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    fs::write(context.run_dir().join("tool-observation.md"), markdown).with_context(|| {
        format!(
            "failed to write tool observation report for run {}",
            context.run_id()
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ToolAsPerceptionCalculatorExperiment, execute_tool_request};
    use crate::experiments::Experiment;
    use crate::runtime::run_context::RunContext;
    use crate::tools::{ToolRegistry, ToolRequest};

    #[test]
    fn tool_experiment_records_tool_and_context_artifacts() {
        let base_dir = std::env::temp_dir().join(format!("qsf-phase5-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "phase-five-test").unwrap();
        let experiment = ToolAsPerceptionCalculatorExperiment;

        let outcome = experiment.run(&mut context).unwrap();

        assert_eq!(context.event_count(), 6);
        assert_eq!(context.trace_count(), 2);
        assert!(outcome.summary.contains("Phase 5"));
        assert!(
            fs::read_to_string(context.run_dir().join("tool-observation.md"))
                .unwrap()
                .contains("Calculator observed")
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn tool_failures_record_tool_failed_event() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-phase5-fail-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "phase-five-failure-test").unwrap();
        let registry = ToolRegistry::default();
        let request = ToolRequest::calculator("2 + (3 *", "test");

        let error = execute_tool_request(&mut context, &registry, &request).unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();

        assert!(
            error.to_string().contains("expected number")
                || error.to_string().contains("expected closing ')'")
        );
        assert_eq!(context.event_count(), 1);
        assert!(events.contains("ToolFailed"));
        assert!(events.contains("calculator"));

        fs::remove_dir_all(base_dir).unwrap();
    }
}
