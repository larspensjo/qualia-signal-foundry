use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::console::styling::ColorMode;
use crate::context::ContextAssembly;
use crate::conversation::prompt::{self, PromptTurn};
use crate::conversation::{PromptAssembly, PromptTurnSummary};
use crate::memory::{
    Association, MemoryFixture, MemoryRecord, RetrievalResult, RetrievalStrategy,
    retrieve_memories, retrieved_memory_ids,
};
use crate::models::{ModelClient, build_client, requested_provider_from_env};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::session::ageing::{run_session_end_flush, sanitize_error};
use crate::session::{
    LiveSessionEvent, SessionBootRequest, SessionConfig, SessionEndReason, SessionEvent,
    SessionState, StateDirectoryResolution, is_turn_summarized,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

mod config;
mod console;
mod report;
mod tool_runtime;
mod turn_runtime;

#[allow(unused_imports)]
pub(crate) use crate::session::config::DEFAULT_SESSION_MODEL;
#[allow(unused_imports)]
pub(crate) use config::{
    DEFAULT_TURN_MAX_OUTPUT_TOKENS, MissingFileSessionMemorySource,
    build_session_memory_source_from_env, parse_turn_max_output_tokens,
    turn_max_output_tokens_from_env,
};
pub(crate) use console::{
    begin_user_input_echo, end_user_input_echo, print_assistant_response, print_memory_blocks,
};
#[allow(unused_imports)]
pub(crate) use report::{prompt_prefix_status_for_report, write_multi_turn_report};
pub(crate) use tool_runtime::{
    conversational_responder_role_with_session_and_project_doc_tools, execute_model_tool_calls,
    project_doc_service_for_multi_turn_text_loop, prompt_tool_message_from_recall,
    responder_request_for_messages,
};
pub(crate) use turn_runtime::{TurnConsole, TurnRequest, run_one_turn};
const SESSION_RETRIEVAL_LIMIT: usize = 8;
const SESSION_RETRIEVAL_STRATEGY: RetrievalStrategy = RetrievalStrategy::KeywordTag;
pub(crate) const MAX_RESPONDER_TOOL_ROUNDS_PER_TURN: usize = 2;
const NON_REPLAYED_TOOL_PROMPT_PREFIX_INVALIDATION: &str = "non_replayed_tool_messages";

pub struct MultiTurnTextLoopExperiment;

impl Experiment for MultiTurnTextLoopExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::MultiTurnTextLoop
    }

    fn description(&self) -> &'static str {
        "Run a human-driven text conversation with append-only session state and cache-stable prompt assembly"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let config = SessionConfig::from_env();
        let state_resolution = crate::session::resolve_shared_state_directory_from_env();
        let model_client = build_client(requested_provider_from_env())?;
        let memory_source = build_session_memory_source_from_env();
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        run_with_io_and_components_at_state_resolution(
            context,
            stdin.lock(),
            &mut stdout,
            model_client.as_ref(),
            memory_source.as_ref(),
            config,
            state_resolution,
        )
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn run_with_io_and_components(
    context: &mut RunContext,
    mut input: impl BufRead,
    output: &mut impl Write,
    model_client: &dyn ModelClient,
    memory_source: &dyn SessionMemorySource,
    config: SessionConfig,
) -> anyhow::Result<ExperimentOutcome> {
    let state_dir = context.run_dir().join("state/text-loop");
    run_with_io_and_components_at_state_resolution(
        context,
        &mut input,
        output,
        model_client,
        memory_source,
        config,
        StateDirectoryResolution {
            resume_state_dir: state_dir.clone(),
            persist_state_dir: state_dir,
            legacy_fallback_used: false,
        },
    )
}

pub(crate) fn run_with_io_and_components_at_state_resolution(
    context: &mut RunContext,
    mut input: impl BufRead,
    output: &mut impl Write,
    model_client: &dyn ModelClient,
    memory_source: &dyn SessionMemorySource,
    config: SessionConfig,
    state_resolution: StateDirectoryResolution,
) -> anyhow::Result<ExperimentOutcome> {
    let boot = crate::session::boot_session(
        context,
        SessionBootRequest {
            resume_state_dir: state_resolution.resume_state_dir.clone(),
            persist_state_dir: state_resolution.persist_state_dir.clone(),
            config,
            legacy_fallback_used: state_resolution.legacy_fallback_used,
        },
    )?;
    let resume_inputs = boot.resume_inputs;
    let mut pending_boot_brief = boot.pending_boot_brief;
    let mut state = boot.state;
    let resume_state_dir = state_resolution.resume_state_dir.clone();
    let persist_state_dir = state_resolution.persist_state_dir.clone();
    let mut memory_snapshot = load_session_memory_snapshot(
        context,
        memory_source,
        &resume_state_dir,
        &persist_state_dir,
    )?;
    write_memory_source_snapshot(context, &memory_snapshot)?;
    let color_mode = ColorMode::for_stdout();
    writeln!(output, "multi-turn-text-loop ready; type :quit to exit")?;

    let mut line = String::new();
    loop {
        line.clear();
        begin_user_input_echo(output, color_mode)?;
        let bytes_read = input.read_line(&mut line);
        end_user_input_echo(output, color_mode)?;
        let bytes_read = bytes_read?;
        if bytes_read == 0 {
            end_session(
                context,
                &mut state,
                &persist_state_dir,
                output,
                color_mode,
                SessionEndReason::Eof,
            )?;
            break;
        }

        let user_input = line.trim_end_matches(['\r', '\n']).to_string();
        if user_input.trim() == ":quit" {
            end_session(
                context,
                &mut state,
                &persist_state_dir,
                output,
                color_mode,
                SessionEndReason::QuitCommand,
            )?;
            break;
        }
        if user_input.trim().is_empty() {
            continue;
        }

        apply_session_event(
            context,
            &mut state,
            SessionEvent::InputReceived {
                input: user_input.clone(),
            },
        )?;

        if completed_turn_count(&state) >= state.config.max_turns {
            let current = completed_turn_count(&state);
            let max = state.config.max_turns;
            let override_active = state.config.allow_over_limit;
            apply_session_event(
                context,
                &mut state,
                SessionEvent::SessionLimitReached {
                    current,
                    max,
                    override_active,
                },
            )?;
            if !override_active {
                writeln!(
                    output,
                    "session limit reached; type :quit or restart with QSF_SESSION_ALLOW_OVER_LIMIT=true"
                )?;
                continue;
            }
        }

        let boot_brief_fragment = if completed_turn_count(&state) == 0 {
            pending_boot_brief
                .take()
                .map(|brief| format_boot_brief_for_context(&brief))
        } else {
            None
        };

        match run_one_turn(
            context,
            &mut state,
            &persist_state_dir,
            &mut memory_snapshot,
            model_client,
            TurnRequest {
                user_input: &user_input,
                boot_brief_fragment,
                max_output_tokens: turn_max_output_tokens_from_env(),
            },
            TurnConsole { output, color_mode },
        ) {
            Ok(response) => {
                print_assistant_response(output, &response, color_mode)?;
            }
            Err(error) => {
                let error_summary = sanitize_error(&error.to_string());
                apply_session_event(
                    context,
                    &mut state,
                    SessionEvent::ModelRoleFailed {
                        error_summary: error_summary.clone(),
                    },
                )?;
                apply_live_session_event(
                    &mut state,
                    LiveSessionEvent::ModelRoleFailed {
                        error_summary: error_summary.clone(),
                    },
                );
                writeln!(output, "{}", retry_message_for_turn_error(&error_summary))?;
                engine_logging::engine_error!(
                    "multi-turn model call failed: run_id={} error={}",
                    context.run_id(),
                    error_summary
                );
            }
        }
    }

    write_multi_turn_report(context, &state, &memory_snapshot)?;
    persist_continuity_state(
        &state,
        &resume_state_dir,
        &persist_state_dir,
        &resume_inputs.manifest,
    )?;

    Ok(ExperimentOutcome {
        summary: format!(
            "The multi-turn text loop completed with {} appended turns, {} warm summaries, {} recall tool executions, and cache-stable prompt hashes recorded per turn.",
            completed_turn_count(&state),
            state.summarized_turns.len(),
            state.turns.iter().map(|turn| turn.recalled_turns.len()).sum::<usize>()
        ),
        observations: vec![
            "Session turns are append-only; older turns can age into stable warm summaries while completed turn records remain available for reporting.".to_string(),
            "Each turn retrieves memory from the latest user input only, then assembles selected fragments under the existing context budget.".to_string(),
            "The recall_turn tool can expand summarized turns into verbatim tool messages that are frozen into future prompt prefixes.".to_string(),
        ],
        failure_modes: vec![
            "Warm summarization invalidates the prompt cache prefix once per ageing event.".to_string(),
            "OpenAI prompt caching reports zero cached tokens below the 1024 input-token floor.".to_string(),
        ],
        follow_up_questions: vec![
            "Should the warm tier summarize by token pressure in addition to turn count?".to_string(),
            "Should later retrieval include recent session turns as query context?".to_string(),
        ],
        decision_candidates: vec![
            "Keep warm-tier summaries session-local and append-only unless explicitly promoted through the reviewed-memory pipeline.".to_string(),
        ],
        extra_artifacts: vec![
            "multi-turn-text-loop.md".to_string(),
            "session-memory-source.json".to_string(),
        ],
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn run_with_io_and_components_at_state_dir(
    context: &mut RunContext,
    input: impl BufRead,
    output: &mut impl Write,
    model_client: &dyn ModelClient,
    memory_source: &dyn SessionMemorySource,
    config: SessionConfig,
    state_dir: impl AsRef<Path>,
) -> anyhow::Result<ExperimentOutcome> {
    let state_dir = state_dir.as_ref().to_path_buf();
    run_with_io_and_components_at_state_resolution(
        context,
        input,
        output,
        model_client,
        memory_source,
        config,
        StateDirectoryResolution {
            resume_state_dir: state_dir.clone(),
            persist_state_dir: state_dir,
            legacy_fallback_used: false,
        },
    )
}

fn completed_turn_count(state: &SessionState) -> usize {
    state.turns.len()
}

fn assemble_session_prompt(
    state: &SessionState,
    user_input: &str,
    retrieved_memory_block: &str,
    project_doc_channel_enabled: bool,
) -> PromptAssembly {
    let summarized_turns = state
        .summarized_turns
        .iter()
        .map(|summary| PromptTurnSummary {
            turn_index: summary.turn_index,
            summary: &summary.summary,
        })
        .collect::<Vec<_>>();
    let prior_turns = state
        .turns
        .iter()
        .filter(|turn| !is_turn_summarized(state, turn.index))
        .map(|turn| PromptTurn {
            user_input: &turn.user_input,
            retrieved_memory_block: &turn.retrieved_memory_block,
            recalled_tool_messages: turn
                .recalled_turns
                .iter()
                .map(prompt_tool_message_from_recall)
                .collect(),
            assistant_response: &turn.assistant_response,
        })
        .collect::<Vec<_>>();

    prompt::assemble_prompt_with_summaries_and_project_doc_channel(
        &summarized_turns,
        &prior_turns,
        user_input,
        retrieved_memory_block,
        project_doc_channel_enabled,
    )
}

fn format_boot_brief_for_context(brief: &crate::sleep::commit::ConsolidatedBrief) -> String {
    crate::session::format_boot_brief_for_context(brief)
}

fn verify_prompt_prefix(
    state: &SessionState,
    prompt_assembly: &PromptAssembly,
) -> anyhow::Result<()> {
    if state.prefix_invalidated_since_last_prompt {
        return Ok(());
    }

    if let Some(previous_turn) = state.turns.last() {
        let prefix_hash = prompt::prior_request_prefix_hash(
            &prompt_assembly.messages,
            previous_turn.message_count,
        )
        .with_context(|| {
            format!(
                "prompt continuity error before turn {}: new prompt did not contain the previous request prefix",
                completed_turn_count(state)
            )
        })?;
        anyhow::ensure!(
            prefix_hash == previous_turn.full_request_hash,
            "prompt continuity error before turn {}: prompt prefix hash mismatch",
            completed_turn_count(state)
        );
    }

    Ok(())
}

fn retry_message_for_turn_error(error_summary: &str) -> &'static str {
    if error_summary.contains("prompt continuity error") {
        "runtime prompt continuity error; see engine.log, then try again or :quit"
    } else {
        "model unavailable, try again or :quit"
    }
}

fn apply_session_event(
    context: &mut RunContext,
    state: &mut SessionState,
    event: SessionEvent,
) -> anyhow::Result<()> {
    crate::session::apply_session_event(context, state, event)
}

fn apply_live_session_event(state: &mut SessionState, event: LiveSessionEvent) {
    crate::session::apply_live_session_event(state, event);
}

fn end_session<W: Write>(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    output: &mut W,
    color_mode: ColorMode,
    reason: SessionEndReason,
) -> anyhow::Result<()> {
    let _ = run_session_end_flush(context, state, state_dir, output, color_mode)?;
    apply_live_session_event(
        state,
        LiveSessionEvent::SessionEnded {
            reason: reason.clone(),
        },
    );
    apply_session_event(context, state, SessionEvent::SessionEnded { reason })
}

fn persist_continuity_state(
    state: &SessionState,
    resume_state_dir: &Path,
    persist_state_dir: &Path,
    previous_manifest: &crate::session::manifest::ContinuityManifest,
) -> anyhow::Result<()> {
    crate::session::persist_continuity_state_from_dirs(
        state,
        resume_state_dir,
        persist_state_dir,
        previous_manifest,
    )
}

pub(crate) trait SessionMemorySource {
    fn load(&self, context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot>;
}

fn load_session_memory_snapshot(
    context: &mut RunContext,
    memory_source: &dyn SessionMemorySource,
    resume_state_dir: &Path,
    persist_state_dir: &Path,
) -> anyhow::Result<SessionMemorySourceSnapshot> {
    let persist_memory_store_path = persist_state_dir.join("memory-store.json");
    if persist_memory_store_path.exists() {
        let store = crate::memory::MemoryStore::load_or_empty(&persist_memory_store_path)?;
        return Ok(SessionMemorySourceSnapshot::from_memory_store(
            &persist_memory_store_path,
            store.contents().clone(),
        ));
    }

    let resume_memory_store_path = resume_state_dir.join("memory-store.json");
    if resume_memory_store_path.exists() {
        let store = crate::memory::MemoryStore::load_or_empty(&resume_memory_store_path)?;
        return Ok(SessionMemorySourceSnapshot::from_memory_store(
            &resume_memory_store_path,
            store.contents().clone(),
        ));
    }

    memory_source.load(context)
}

/// Reload-on-change snapshot refresh. Called after persistence that may have
/// introduced or strengthened associations.
fn reload_session_memory_source_snapshot(
    memory_store_path: &Path,
) -> anyhow::Result<SessionMemorySourceSnapshot> {
    let store = crate::memory::MemoryStore::load_or_empty(memory_store_path)?;
    Ok(SessionMemorySourceSnapshot::from_memory_store(
        memory_store_path,
        store.contents().clone(),
    ))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SessionMemorySourceSnapshot {
    source_name: String,
    source_reference: String,
    records: Vec<MemoryRecord>,
    associations: Vec<Association>,
}

impl SessionMemorySourceSnapshot {
    fn from_fixture(
        source_name: impl Into<String>,
        source_reference: impl Into<String>,
        fixture: MemoryFixture,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            source_reference: source_reference.into(),
            records: fixture.records,
            associations: fixture.associations,
        }
    }

    fn from_memory_store(path: &Path, contents: crate::memory::MemoryStoreContents) -> Self {
        Self {
            source_name: "memory_store".to_string(),
            source_reference: path.display().to_string(),
            records: contents.records,
            associations: contents.associations,
        }
    }
}

fn write_memory_source_snapshot(
    context: &RunContext,
    snapshot: &SessionMemorySourceSnapshot,
) -> anyhow::Result<()> {
    fs::write(
        context.run_dir().join("session-memory-source.json"),
        serde_json::to_string_pretty(snapshot)?,
    )
    .context("failed to write session memory source snapshot")
}

fn retrieve_session_memories(
    context: &mut RunContext,
    state: &SessionState,
    memory_snapshot: &SessionMemorySourceSnapshot,
    query: &str,
) -> anyhow::Result<RetrievalResult> {
    context.record_event(
        EventType::MemoryRetrievalRequested,
        json!({
            "session_id": context.run_id(),
            "turn_index": completed_turn_count(state),
            "query": query,
            "strategy": SESSION_RETRIEVAL_STRATEGY,
            "retrieval_limit": SESSION_RETRIEVAL_LIMIT,
            "memory_source": &memory_snapshot.source_name,
            "memory_source_reference": &memory_snapshot.source_reference,
        }),
        None,
    )?;
    let retrieval = retrieve_memories(
        &memory_snapshot.records,
        &memory_snapshot.associations,
        query,
        SESSION_RETRIEVAL_STRATEGY,
        SESSION_RETRIEVAL_LIMIT,
    )?;
    let trace = TraceRecord::new(
        context.experiment_id(),
        "session-memory-retrieval",
        format!("turn={} query={}", completed_turn_count(state), query),
        format!(
            "selected {} and omitted {} memory candidates",
            retrieval.selected.len(),
            retrieval.omitted.len()
        ),
    )
    .with_details(json!({
        "session_id": context.run_id(),
        "turn_index": completed_turn_count(state),
        "retrieval": &retrieval,
        "memory_source": &memory_snapshot.source_name,
    }))
    .with_latency_context("runtime", "session-memory-retrieval")
    .with_latency_ns(retrieval.latency_ns);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    context.record_event(
        EventType::MemoryRetrieved,
        json!({
            "session_id": context.run_id(),
            "turn_index": completed_turn_count(state),
            "memory_source": &memory_snapshot.source_name,
            "strategy": retrieval.strategy,
            "selected": retrieved_memory_ids(&retrieval.selected),
            "omitted": retrieved_memory_ids(&retrieval.omitted),
            "latency_ms": retrieval.latency_ms,
            "latency_ns": retrieval.latency_ns,
        }),
        Some(trace_id),
    )?;

    Ok(retrieval)
}

fn record_context_assembly(
    context: &mut RunContext,
    state: &SessionState,
    assembly: &ContextAssembly,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "session-context-assembly",
        format!(
            "turn={} retrieved memory fragments",
            completed_turn_count(state)
        ),
        format!(
            "selected {} fragments and omitted {}",
            assembly.selected.len(),
            assembly.omitted.len()
        ),
    )
    .with_details(json!({
        "session_id": context.run_id(),
        "turn_index": completed_turn_count(state),
        "assembly": assembly,
    }))
    .with_latency_context("runtime", "session-context-assembly")
    .with_latency_ms(0);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

#[cfg(test)]
mod tests;
