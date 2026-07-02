use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

use serde_json::json;

use crate::console::styling::{ColorMode, STYLE_DROP_MARKER, paint};
use crate::models::invoke_model_role;
use crate::observability::event_log::EventType;
use crate::observability::trace::elapsed_ms;
use crate::runtime::run_context::RunContext;
use qsf_models::{ModelClient, ModelMessage, ModelRequest, ModelRole, ModelRoleId};

use super::{SessionEvent, SessionState, Turn, TurnRange, TurnSummary, apply_session_event};

const HOT_HIGH_WATER_FRACTION: f64 = 0.80;
const HOT_LOW_WATER_FRACTION: f64 = 0.50;
pub(crate) const WARM_SUMMARY_MAX_OUTPUT_TOKENS: u32 = 80;
pub(crate) const WARM_SUMMARY_RETRY_MAX_OUTPUT_TOKENS: u32 = 240;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CrossTurnPersistOutcome {
    pub new_associations: usize,
    pub strengthened: usize,
}

pub(crate) struct CrossTurnPersistRequest<'a> {
    pub(crate) first_turn_index: usize,
    pub(crate) last_turn_index: usize,
    pub(crate) kind: qsf_memory::ProcessedRangeKind,
    pub(crate) now: time::OffsetDateTime,
    pub(crate) event_kind: &'a str,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DropOutcome {
    pub aged_count: usize,
    pub new_associations: usize,
    pub strengthened: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TokenBudgetDropPlan {
    pub first_turn_index: usize,
    pub last_turn_index: usize,
    pub aged_count: usize,
    pub hot_tokens_before: usize,
    pub hot_tokens_after: usize,
}

fn turn_verbatim_estimated_tokens(turn: &Turn) -> usize {
    let total_chars = turn.user_input.chars().count()
        + turn.retrieved_memory_block.chars().count()
        + turn.assistant_response.chars().count();
    total_chars.div_ceil(4).max(1)
}

pub(crate) fn plan_token_budget_drop(
    state: &SessionState,
    model_window: usize,
    high_water_fraction: f64,
    low_water_fraction: f64,
) -> Option<TokenBudgetDropPlan> {
    let active_start = state.summarized_turns.len();
    let active_turns = state.turns.iter().skip(active_start).collect::<Vec<_>>();
    if active_turns.is_empty() {
        return None;
    }

    let per_turn = active_turns
        .iter()
        .map(|turn| turn_verbatim_estimated_tokens(turn))
        .collect::<Vec<_>>();
    let hot_tokens_before = per_turn.iter().sum::<usize>();
    let high_water = (model_window as f64 * high_water_fraction) as usize;
    if hot_tokens_before <= high_water {
        return None;
    }

    let low_water = (model_window as f64 * low_water_fraction) as usize;
    let mut tokens = hot_tokens_before;
    let mut aged_count = 0;
    for size in &per_turn {
        if tokens <= low_water {
            break;
        }
        tokens = tokens.saturating_sub(*size);
        aged_count += 1;
    }

    if aged_count == 0 {
        return None;
    }

    Some(TokenBudgetDropPlan {
        first_turn_index: active_start,
        last_turn_index: active_start + aged_count - 1,
        aged_count,
        hot_tokens_before,
        hot_tokens_after: tokens,
    })
}

fn completed_turn_count(state: &SessionState) -> usize {
    state.turns.len()
}

fn active_turn_count(state: &SessionState) -> usize {
    state
        .turns
        .len()
        .saturating_sub(state.summarized_turns.len())
}

fn oldest_unsummarized_turn(state: &SessionState) -> Option<&Turn> {
    state.turns.get(state.summarized_turns.len())
}

pub(crate) fn age_out_warm_turns(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    model_client: &dyn ModelClient,
) -> anyhow::Result<()> {
    while active_turn_count(state) > state.config.warm_threshold {
        let Some(turn) = oldest_unsummarized_turn(state).cloned() else {
            break;
        };
        if let Err(error) = persist_cross_turn_range(
            context,
            state,
            &state_dir.join("memory-store.json"),
            CrossTurnPersistRequest {
                first_turn_index: turn.index,
                last_turn_index: turn.index,
                kind: qsf_memory::ProcessedRangeKind::LiveBatch,
                now: time::OffsetDateTime::now_utc(),
                event_kind: "live_count_threshold",
            },
        ) {
            let error_summary = sanitize_error(&error.to_string());
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "session_id": state.session_id,
                    "stage": "live-count-threshold-cross-turn",
                    "state_dir": state_dir.display().to_string(),
                    "turn_index": turn.index,
                    "error": error_summary,
                }),
                None,
            )?;
            engine_logging::engine_error!(
                "count-threshold cross-turn persistence failed: session_id={} state_dir={} turn_index={} error={}",
                state.session_id,
                state_dir.display(),
                turn.index,
                error_summary
            );
        }

        let summary = match summarize_turn_with_retry(context, state, model_client, &turn) {
            Ok(summary) => summary,
            Err(error) => {
                let error_summary = sanitize_error(&error.to_string());
                context.record_event(
                    EventType::ErrorOccurred,
                    json!({
                        "session_id": state.session_id,
                        "stage": "session-turn-summarization",
                        "turn_index": turn.index,
                        "error": error_summary,
                    }),
                    None,
                )?;
                engine_logging::engine_error!(
                    "multi-turn summarization failed: session_id={} run_id={} turn_index={} error={}",
                    state.session_id,
                    context.run_id(),
                    turn.index,
                    error_summary
                );
                break;
            }
        };

        apply_session_event(context, state, SessionEvent::TurnSummarized(summary))?;
    }

    Ok(())
}

pub(crate) fn maybe_run_token_budget_drop<W: Write>(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    model_client: &dyn ModelClient,
    output: &mut W,
    color_mode: ColorMode,
) -> anyhow::Result<Option<DropOutcome>> {
    let (window, known_model_window) =
        crate::runtime::model_context_window::model_max_tokens_or_default(&state.config.model_id);
    if !known_model_window {
        context.record_event(
            EventType::ErrorOccurred,
            json!({
                "session_id": state.session_id,
                "stage": "token-budget-aging-model-window",
                "model_id": state.config.model_id,
                "fallback_max_tokens": window,
                "message": "unknown model context window; using fallback for token-budget aging",
            }),
            None,
        )?;
    }

    let Some(plan) = plan_token_budget_drop(
        state,
        window,
        HOT_HIGH_WATER_FRACTION,
        HOT_LOW_WATER_FRACTION,
    ) else {
        return Ok(None);
    };
    let first_turn_index = plan.first_turn_index;
    let last_turn_index = plan.last_turn_index;

    let event = match run_token_budget_drop_side_effect(
        context,
        state,
        state_dir,
        plan,
        time::OffsetDateTime::now_utc(),
        model_client,
    ) {
        Ok(event) => event,
        Err(error) => {
            let error_summary = sanitize_error(&error.to_string());
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "session_id": state.session_id,
                    "stage": "token-budget-aging-summary",
                    "state_dir": state_dir.display().to_string(),
                    "first_turn_index": first_turn_index,
                    "last_turn_index": last_turn_index,
                    "error": error_summary,
                }),
                None,
            )?;
            engine_logging::engine_error!(
                "token-budget aging summary failed: session_id={} state_dir={} range={}..={} error={}",
                state.session_id,
                state_dir.display(),
                first_turn_index,
                last_turn_index,
                error_summary
            );
            return Ok(None);
        }
    };
    let outcome = drop_outcome_from_event(&event);
    apply_session_event(context, state, event)?;

    if let Some(outcome) = &outcome {
        print_drop_marker(
            output,
            outcome.aged_count,
            outcome.new_associations,
            outcome.strengthened,
            color_mode,
        )?;
    }

    Ok(outcome)
}

pub(crate) fn run_token_budget_drop_side_effect(
    context: &mut RunContext,
    state: &SessionState,
    state_dir: &Path,
    plan: TokenBudgetDropPlan,
    now: time::OffsetDateTime,
    model_client: &dyn ModelClient,
) -> anyhow::Result<SessionEvent> {
    let store_path = state_dir.join("memory-store.json");
    anyhow::ensure!(
        plan.first_turn_index <= plan.last_turn_index,
        "token-budget aging received inverted range: first_turn_index={} last_turn_index={}",
        plan.first_turn_index,
        plan.last_turn_index
    );

    let persist = persist_cross_turn_range(
        context,
        state,
        &store_path,
        CrossTurnPersistRequest {
            first_turn_index: plan.first_turn_index,
            last_turn_index: plan.last_turn_index,
            kind: qsf_memory::ProcessedRangeKind::LiveBatch,
            now,
            event_kind: "live_batch",
        },
    )?
    .unwrap_or_default();

    let summaries = summarize_aged_turns(
        context,
        state,
        plan.first_turn_index,
        plan.last_turn_index,
        model_client,
    )?;

    Ok(SessionEvent::TurnsAgedAndCoRetrieved {
        range: TurnRange {
            first_index: plan.first_turn_index,
            last_index: plan.last_turn_index,
        },
        new_associations: persist.new_associations,
        strengthened_associations: persist.strengthened,
        persisted_at: SystemTime::now(),
        summaries,
    })
}

pub(crate) fn run_session_end_flush<W: Write>(
    context: &mut RunContext,
    state: &SessionState,
    state_dir: &Path,
    output: &mut W,
    color_mode: ColorMode,
) -> anyhow::Result<Option<DropOutcome>> {
    use crate::memory::processed_ranges::uncovered_turn_indices;

    let store_path = state_dir.join("memory-store.json");
    if !store_path.exists() {
        return Ok(None);
    }

    let store = crate::memory::MemoryStore::load_or_empty(&store_path)?;
    let active_start = state.summarized_turns.len();
    let total = state.turns.len();
    if total == 0 || active_start >= total {
        return Ok(None);
    }

    let uncovered = uncovered_turn_indices(
        &store.contents().processed_ranges,
        &state.session_id,
        active_start,
        total - 1,
    );
    if uncovered.is_empty() {
        return Ok(None);
    }

    let now = time::OffsetDateTime::now_utc();
    let persist = match persist_cross_turn_range(
        context,
        state,
        &store_path,
        CrossTurnPersistRequest {
            first_turn_index: active_start,
            last_turn_index: total - 1,
            kind: qsf_memory::ProcessedRangeKind::SessionEnd,
            now,
            event_kind: "session_end",
        },
    ) {
        Ok(Some(persist)) => persist,
        Ok(None) => return Ok(None),
        Err(error) => {
            let error_summary = sanitize_error(&error.to_string());
            engine_logging::engine_error!(
                "session-end flush persist failed; deferring to sleep safety net: session_id={} state_dir={} range={}..={} error={}",
                state.session_id,
                state_dir.display(),
                active_start,
                total - 1,
                error_summary
            );
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "session_id": state.session_id,
                    "stage": "session-end-cross-turn-flush",
                    "state_dir": state_dir.display().to_string(),
                    "first_turn_index": active_start,
                    "last_turn_index": total - 1,
                    "error": error_summary,
                }),
                None,
            )?;
            return Ok(None);
        }
    };

    let outcome = DropOutcome {
        aged_count: uncovered.len(),
        new_associations: persist.new_associations,
        strengthened: persist.strengthened,
    };
    print_session_end_flush(
        output,
        outcome.new_associations,
        outcome.strengthened,
        color_mode,
    )?;

    Ok(Some(outcome))
}

fn drop_outcome_from_event(event: &SessionEvent) -> Option<DropOutcome> {
    match event {
        SessionEvent::TurnsAgedAndCoRetrieved {
            range,
            new_associations,
            strengthened_associations,
            ..
        } => Some(DropOutcome {
            aged_count: range.last_index + 1 - range.first_index,
            new_associations: *new_associations,
            strengthened: *strengthened_associations,
        }),
        _ => None,
    }
}

pub(crate) fn persist_cross_turn_range(
    context: &mut RunContext,
    state: &SessionState,
    store_path: &Path,
    request: CrossTurnPersistRequest<'_>,
) -> anyhow::Result<Option<CrossTurnPersistOutcome>> {
    use crate::memory::co_retrieval::{
        CROSS_TURN_ASSOCIATION_WINDOW, CoRetrievalDelta, CrossTurnAnchorRange,
        generate_cross_turn_deltas_for_anchor_ranges,
    };
    use crate::memory::processed_ranges::{contiguous_ranges, uncovered_turn_indices};
    use qsf_memory::ProcessedRange;

    if !store_path.exists() || state.turns.is_empty() {
        return Ok(None);
    }

    let mut store = crate::memory::MemoryStore::load_or_empty(store_path)?;
    let last_requested = request
        .last_turn_index
        .min(state.turns.len().saturating_sub(1));
    if request.first_turn_index > last_requested {
        return Ok(Some(CrossTurnPersistOutcome::default()));
    }
    let uncovered = uncovered_turn_indices(
        &store.contents().processed_ranges,
        &state.session_id,
        request.first_turn_index,
        last_requested,
    );
    if uncovered.is_empty() {
        return Ok(Some(CrossTurnPersistOutcome::default()));
    }
    let ranges = contiguous_ranges(&uncovered);
    let anchor_ranges = ranges
        .iter()
        .map(|(first, last)| CrossTurnAnchorRange {
            first_turn: *first,
            last_turn: *last,
        })
        .collect::<Vec<_>>();
    let retrievals = state
        .turns
        .iter()
        .map(|turn| turn.context_assembly.retrieved_memory_ids())
        .collect::<Vec<_>>();
    let known_record_ids = store
        .contents()
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect::<HashSet<_>>();
    let deltas = generate_cross_turn_deltas_for_anchor_ranges(
        &retrievals,
        &store.contents().associations,
        &known_record_ids,
        CROSS_TURN_ASSOCIATION_WINDOW,
        &state.session_id,
        request.now,
        &anchor_ranges,
    );

    let mut outcome = CrossTurnPersistOutcome::default();
    for delta in deltas {
        match delta {
            CoRetrievalDelta::Create {
                from,
                to,
                weight,
                reason,
                at,
            } => {
                store
                    .contents_mut()
                    .associations
                    .push(crate::memory::Association::new(
                        from, to, weight, reason, at,
                    ));
                outcome.new_associations += 1;
            }
            CoRetrievalDelta::Strengthen {
                from,
                to,
                new_weight,
                at,
            } => {
                if let Some(existing) =
                    store
                        .contents_mut()
                        .associations
                        .iter_mut()
                        .find(|association| {
                            (association.from_memory_id == from && association.to_memory_id == to)
                                || (association.from_memory_id == to
                                    && association.to_memory_id == from)
                        })
                {
                    existing.weight = new_weight;
                    existing.last_reinforced_at = at;
                    outcome.strengthened += 1;
                }
            }
        }
    }

    store
        .contents_mut()
        .processed_ranges
        .extend(ranges.iter().map(|(first, last)| ProcessedRange {
            session_id: state.session_id.clone(),
            first_turn_index: *first,
            last_turn_index: *last,
            kind: request.kind.clone(),
            at: request.now,
        }));
    store.persist()?;

    context.record_event(
        EventType::CoRetrievalAssociationsProposed,
        json!({
            "session_id": state.session_id,
            "kind": request.event_kind,
            "state_dir": store_path.parent().map(|path| path.display().to_string()),
            "first_turn_index": request.first_turn_index,
            "last_turn_index": request.last_turn_index,
            "processed_ranges": ranges,
            "processed_anchor_count": uncovered.len(),
            "new_count": outcome.new_associations,
            "strengthened_count": outcome.strengthened,
            "aged_turn_count": request.last_turn_index + 1 - request.first_turn_index,
        }),
        None,
    )?;
    context.record_event(
        EventType::MemoryStorePersisted,
        json!({
            "session_id": state.session_id,
            "stage": request.event_kind,
            "path": store_path.display().to_string(),
            "records_count": store.contents().records.len(),
            "associations_count": store.contents().associations.len(),
            "processed_ranges_count": store.contents().processed_ranges.len(),
        }),
        None,
    )?;

    Ok(Some(outcome))
}

pub(crate) fn summarize_aged_turns(
    context: &mut RunContext,
    state: &SessionState,
    first_turn_index: usize,
    last_turn_index: usize,
    model_client: &dyn ModelClient,
) -> anyhow::Result<Vec<TurnSummary>> {
    if last_turn_index < first_turn_index {
        return Ok(Vec::new());
    }

    let mut summaries = Vec::with_capacity(last_turn_index + 1 - first_turn_index);
    for index in first_turn_index..=last_turn_index {
        let turn = &state.turns[index];
        summaries.push(summarize_turn_with_retry(
            context,
            state,
            model_client,
            turn,
        )?);
    }

    Ok(summaries)
}

fn summarize_turn_with_retry(
    context: &mut RunContext,
    state: &SessionState,
    model_client: &dyn ModelClient,
    turn: &Turn,
) -> anyhow::Result<TurnSummary> {
    let (summary, finish_reason) = summarize_turn_once(
        context,
        state,
        model_client,
        turn,
        WARM_SUMMARY_MAX_OUTPUT_TOKENS,
        false,
    )?;
    if is_summary_truncated_finish_reason(finish_reason.as_deref()) {
        let (retry_summary, retry_finish_reason) = summarize_turn_once(
            context,
            state,
            model_client,
            turn,
            WARM_SUMMARY_RETRY_MAX_OUTPUT_TOKENS,
            true,
        )?;
        if is_summary_truncated_finish_reason(retry_finish_reason.as_deref()) {
            anyhow::bail!(
                "session turn summarizer truncated after retry: session_id={} turn_index={} finish_reason={:?}",
                state.session_id,
                turn.index,
                retry_finish_reason
            );
        }
        return Ok(retry_summary);
    }

    Ok(summary)
}

fn summarize_turn_once(
    context: &mut RunContext,
    state: &SessionState,
    model_client: &dyn ModelClient,
    turn: &Turn,
    max_output_tokens: u32,
    is_retry: bool,
) -> anyhow::Result<(TurnSummary, Option<String>)> {
    let mut messages = vec![ModelMessage::system(
        "Summarize exactly one aged-out conversation turn in one sentence. Preserve concrete user intent, assistant commitments, and project-specific facts. Do not add new facts.",
    )];
    if is_retry {
        messages.push(ModelMessage::system(
            "This is a retry because the previous summary was truncated. Produce a complete concise sentence that fits the available output budget and does not end mid-thought.",
        ));
    }
    messages.push(ModelMessage::user(format!(
        "[Turn {}]\n[User]\n{}\n\n[Assistant]\n{}",
        turn.index, turn.user_input, turn.assistant_response
    )));

    let request = ModelRequest::new(
        ModelRole::predefined(ModelRoleId::SessionTurnSummarizer),
        messages,
    )
    .with_session_id(context.run_id())
    .with_temperature(0.0)
    .with_max_output_tokens(max_output_tokens);
    let started_at = std::time::Instant::now();
    let response = invoke_model_role(context, model_client, &request)?;
    let usage = response.usage.as_ref();

    Ok((
        TurnSummary {
            turn_index: turn.index,
            summarized_after_turn_index: completed_turn_count(state) - 1,
            summary: normalize_summary(&response.output_text),
            model_id: response.model_name,
            model_latency_ms: elapsed_ms(started_at),
            input_tokens: usage.map(|usage| usage.input_tokens).unwrap_or(0),
            output_tokens: usage.map(|usage| usage.output_tokens).unwrap_or(0),
        },
        response.finish_reason,
    ))
}

fn is_summary_truncated_finish_reason(finish_reason: Option<&str>) -> bool {
    matches!(finish_reason, Some("max_tokens" | "length"))
}

fn normalize_summary(summary: &str) -> String {
    summary
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub(crate) fn print_drop_marker<W: Write>(
    output: &mut W,
    aged_turn_count: usize,
    new_associations: usize,
    strengthened: usize,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    let line = format!(
        "--- aged {} turns from prompt; +{} associations, *{} strengthened ---",
        aged_turn_count, new_associations, strengthened
    );
    writeln!(output, "{}", paint(color_mode, STYLE_DROP_MARKER, &line))
}

pub(crate) fn print_session_end_flush<W: Write>(
    output: &mut W,
    new_associations: usize,
    strengthened: usize,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    let line = format!(
        "--- session-end flush; +{} associations, *{} strengthened ---",
        new_associations, strengthened
    );
    writeln!(output, "{}", paint(color_mode, STYLE_DROP_MARKER, &line))
}

pub(crate) fn sanitize_error(error: &str) -> String {
    if error.contains("sk-") || error.to_ascii_lowercase().contains("authorization") {
        "provider error redacted because it may contain credential-like content".to_string()
    } else {
        error.to_string()
    }
}
