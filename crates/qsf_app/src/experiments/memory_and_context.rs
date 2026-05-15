use std::fs;
use std::time::Instant;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::context::{ContextBudget, ContextFragment, assemble_context};
use crate::memory::{
    MemoryFixture, RetrievalResult, RetrievalStrategy, phase_four_fixture, retrieve_memories,
    retrieved_memory_ids,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, duration_ms, duration_ns};
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const MEMORY_QUERY: &str =
    "How should associative memory help choose context under a small budget?";
const CONTEXT_QUERY: &str =
    "What context should be carried forward for transcript-first external input?";

pub struct AssociativeMemoryToyModelExperiment;

impl Experiment for AssociativeMemoryToyModelExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::AssociativeMemoryToyModel
    }

    fn description(&self) -> &'static str {
        "Compare recency, keyword/tag, and association-weighted memory retrieval"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = phase_four_fixture();
        write_fixture_snapshot(context, &fixture)?;

        context.record_event(
            EventType::InputReceived,
            json!({
                "query": MEMORY_QUERY,
                "memory_records": fixture.records.len(),
                "associations": fixture.associations.len(),
            }),
            None,
        )?;

        let budget = ContextBudget::new(3, 115);
        let mut runs = Vec::new();

        for strategy in [
            RetrievalStrategy::RecencyOnly,
            RetrievalStrategy::KeywordTag,
            RetrievalStrategy::AssociationWeighted,
        ] {
            let run =
                run_retrieval_and_context(context, &fixture, MEMORY_QUERY, strategy, 5, budget)?;
            runs.push(run);
        }

        write_comparison_report(context, "retrieval-comparison.md", &runs)?;

        Ok(ExperimentOutcome {
            summary: "The associative memory toy model compared recency-only, keyword/tag, and association-weighted retrieval, then applied the same context budget to each strategy.".to_string(),
            observations: vec![
                "Association-weighted retrieval can surface linked context-budget and sleep-phase memories even when they are not the newest records.".to_string(),
                "The retrieval trace exposes score components, matched terms, association paths, and omitted candidates for manual review.".to_string(),
                "Context assembly records both selected and omitted fragments under the same budget across strategies.".to_string(),
            ],
            failure_modes: vec![
                "The fixture is hand-authored, so score behavior may be overfit to the first experiment.".to_string(),
                "Token counts are rough estimates rather than tokenizer-derived measurements.".to_string(),
            ],
            follow_up_questions: vec![
                "Should the first live context manager use association-weighted retrieval by default or keep strategy selection experiment-specific?".to_string(),
                "How should old but high-importance memories compete with recent low-importance observations?".to_string(),
            ],
            decision_candidates: vec![
                "Require memory retrieval traces for every memory experiment.".to_string(),
                "Keep recency-only as the standing baseline for retrieval experiments.".to_string(),
            ],
            extra_artifacts: vec![
                "memory-fixture.json".to_string(),
                "retrieval-comparison.md".to_string(),
            ],
        })
    }
}

pub struct ContextBudgetRetrievalTestExperiment;

impl Experiment for ContextBudgetRetrievalTestExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::ContextBudgetRetrievalTest
    }

    fn description(&self) -> &'static str {
        "Compare selected and omitted memory context under a deliberately small budget"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = phase_four_fixture();
        write_fixture_snapshot(context, &fixture)?;

        context.record_event(
            EventType::InputReceived,
            json!({
                "query": CONTEXT_QUERY,
                "memory_records": fixture.records.len(),
                "associations": fixture.associations.len(),
            }),
            None,
        )?;

        let budget = ContextBudget::new(2, 70);
        let mut runs = Vec::new();

        for strategy in [
            RetrievalStrategy::RecencyOnly,
            RetrievalStrategy::KeywordTag,
            RetrievalStrategy::AssociationWeighted,
        ] {
            let run =
                run_retrieval_and_context(context, &fixture, CONTEXT_QUERY, strategy, 6, budget)?;
            runs.push(run);
        }

        write_comparison_report(context, "context-budget-comparison.md", &runs)?;

        Ok(ExperimentOutcome {
            summary: "The context budget retrieval test forced retrieved memory fragments through a two-fragment, seventy-token budget and logged what was selected and omitted.".to_string(),
            observations: vec![
                "The smaller context budget creates visible tradeoffs between direct relevance, association strength, recency, and token cost.".to_string(),
                "Omitted fragments include explicit reasons, making budget pressure inspectable in traces.".to_string(),
            ],
            failure_modes: vec![
                "The first budget policy is greedy and does not yet try lower-cost combinations after selecting a high-scoring expensive fragment.".to_string(),
                "Manual relevance ratings are not implemented yet; the report is ready for human review but does not score quality automatically.".to_string(),
            ],
            follow_up_questions: vec![
                "Should context assembly optimize for total score under budget instead of greedy score order?".to_string(),
                "Should omitted high-score fragments produce warnings for the model role or only research traces?".to_string(),
            ],
            decision_candidates: vec![
                "Context assembly must log both selected and omitted fragments.".to_string(),
                "The first context manager should keep an explicit fragment and token budget even in prototypes.".to_string(),
            ],
            extra_artifacts: vec![
                "memory-fixture.json".to_string(),
                "context-budget-comparison.md".to_string(),
            ],
        })
    }
}

#[derive(Clone, Debug, Serialize)]
struct StrategyRunSummary {
    strategy: RetrievalStrategy,
    selected_memory_ids: Vec<String>,
    omitted_memory_ids: Vec<String>,
    selected_context_ids: Vec<String>,
    omitted_context_ids: Vec<String>,
    context_tokens_used: usize,
}

fn run_retrieval_and_context(
    context: &mut RunContext,
    fixture: &MemoryFixture,
    query: &str,
    strategy: RetrievalStrategy,
    retrieval_limit: usize,
    budget: ContextBudget,
) -> anyhow::Result<StrategyRunSummary> {
    context.record_event(
        EventType::MemoryRetrievalRequested,
        json!({
            "query": query,
            "strategy": strategy,
            "retrieval_limit": retrieval_limit,
        }),
        None,
    )?;

    let retrieval = retrieve_memories(
        &fixture.records,
        &fixture.associations,
        query,
        strategy,
        retrieval_limit,
    )?;
    let retrieval_trace = retrieval_trace(context.experiment_id(), &retrieval);
    let retrieval_trace_id = retrieval_trace.trace_id;
    context.record_trace(retrieval_trace)?;
    context.record_event(
        EventType::MemoryRetrieved,
        json!({
            "strategy": strategy,
            "selected": retrieved_memory_ids(&retrieval.selected),
            "omitted": retrieved_memory_ids(&retrieval.omitted),
            "latency_ms": retrieval.latency_ms,
            "latency_ns": retrieval.latency_ns,
        }),
        Some(retrieval_trace_id),
    )?;

    let fragments = retrieval
        .selected
        .iter()
        .map(ContextFragment::from)
        .collect::<Vec<_>>();

    context.record_event(
        EventType::ContextAssemblyRequested,
        json!({
            "strategy": strategy,
            "budget": budget,
            "candidate_fragments": fragments.iter().map(|fragment| &fragment.fragment_id).collect::<Vec<_>>(),
        }),
        None,
    )?;

    let started_at = Instant::now();
    let assembly = assemble_context(fragments, budget);
    let elapsed = started_at.elapsed();
    let elapsed_ms = duration_ms(elapsed);
    let elapsed_ns = duration_ns(elapsed);
    let context_trace = TraceRecord::new(
        context.experiment_id(),
        "context-assembly",
        format!(
            "strategy={strategy} budget_fragments={}",
            budget.max_fragments
        ),
        format!(
            "selected={} omitted={} used_estimated_tokens={}",
            assembly.selected.len(),
            assembly.omitted.len(),
            assembly.used_estimated_tokens
        ),
    )
    .with_details(json!({
        "strategy": strategy,
        "budget": budget,
        "selected": assembly.selected,
        "omitted": assembly.omitted,
    }))
    .with_latency_ns(elapsed_ns);
    let context_trace_id = context_trace.trace_id;
    context.record_trace(context_trace)?;
    context.record_event(
        EventType::ContextAssembled,
        json!({
            "strategy": strategy,
            "selected": assembly.selected.iter().map(|selection| &selection.fragment.fragment_id).collect::<Vec<_>>(),
            "omitted": assembly.omitted.iter().map(|omission| &omission.fragment.fragment_id).collect::<Vec<_>>(),
            "used_estimated_tokens": assembly.used_estimated_tokens,
            "latency_ms": elapsed_ms,
            "latency_ns": elapsed_ns,
        }),
        Some(context_trace_id),
    )?;

    Ok(StrategyRunSummary {
        strategy,
        selected_memory_ids: retrieved_memory_ids(&retrieval.selected),
        omitted_memory_ids: retrieved_memory_ids(&retrieval.omitted),
        selected_context_ids: assembly
            .selected
            .iter()
            .map(|selection| selection.fragment.fragment_id.clone())
            .collect(),
        omitted_context_ids: assembly
            .omitted
            .iter()
            .map(|omission| omission.fragment.fragment_id.clone())
            .collect(),
        context_tokens_used: assembly.used_estimated_tokens,
    })
}

fn retrieval_trace(experiment_id: &str, retrieval: &RetrievalResult) -> TraceRecord {
    TraceRecord::new(
        experiment_id,
        "memory-retrieval",
        format!("strategy={} query={}", retrieval.strategy, retrieval.query),
        format!(
            "selected={} omitted={}",
            retrieval.selected.len(),
            retrieval.omitted.len()
        ),
    )
    .with_details(json!({
        "query": retrieval.query,
        "strategy": retrieval.strategy,
        "selected": retrieval.selected,
        "omitted": retrieval.omitted,
    }))
    .with_latency_ns(retrieval.latency_ns)
}

fn write_fixture_snapshot(context: &RunContext, fixture: &MemoryFixture) -> anyhow::Result<()> {
    let path = context.run_dir().join("memory-fixture.json");
    let contents = serde_json::to_string_pretty(fixture)?;
    fs::write(&path, contents).with_context(|| {
        format!(
            "failed to write memory fixture snapshot for run {}",
            context.run_id()
        )
    })?;
    Ok(())
}

fn write_comparison_report(
    context: &RunContext,
    filename: &str,
    runs: &[StrategyRunSummary],
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Retrieval Comparison\n\n");
    markdown.push_str(
        "| Strategy | Selected memories | Selected context | Omitted context | Tokens |\n",
    );
    markdown.push_str("|---|---|---|---|---:|\n");

    for run in runs {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            run.strategy,
            run.selected_memory_ids.join(", "),
            run.selected_context_ids.join(", "),
            run.omitted_context_ids.join(", "),
            run.context_tokens_used
        ));
    }

    fs::write(context.run_dir().join(filename), markdown)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run_retrieval_and_context;
    use crate::context::ContextBudget;
    use crate::memory::{RetrievalStrategy, phase_four_fixture};
    use crate::runtime::run_context::RunContext;

    #[test]
    fn phase_four_run_records_memory_and_context_events() {
        let base_dir = std::env::temp_dir().join(format!("qsf-phase4-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "phase-four-test").unwrap();
        let fixture = phase_four_fixture();

        let summary = run_retrieval_and_context(
            &mut context,
            &fixture,
            "context budget memory",
            RetrievalStrategy::AssociationWeighted,
            5,
            ContextBudget::new(2, 80),
        )
        .unwrap();

        assert_eq!(context.trace_count(), 2);
        assert!(!summary.selected_memory_ids.is_empty());
        assert!(!summary.omitted_context_ids.is_empty());

        std::fs::remove_dir_all(base_dir).unwrap();
    }
}
