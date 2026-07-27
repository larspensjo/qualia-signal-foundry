use std::collections::BTreeMap;
use std::time::SystemTime;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use qsf_diagnostics::{
    DiagnosticRecord, DiagnosticTrust, LiveGoalFormationTrace, RealtimeBoundedInitiativeTrace,
    VolitionContextInjectionTrace, WorldConsultationTrace,
};
use qsf_session::{Exchange, ExchangeInput};

use crate::transcript::model::{
    FormationStatus, FormationView, InitiativeView, MatchView, OrphanCounts, SessionLine,
    SkippedLineView, SourceIntegrity, SurfacedFactView, TraceBundle, TurnContextView, TurnLine,
    VolitionView, WinnerView, WorldView, render_keyword,
};

/// One realtime run: a `session_allocated` record and everything appended before the next one.
/// Source integrity lives inside `header.source` rather than beside it, so it cannot be computed
/// and then omitted from the emitted artifact.
#[derive(Debug)]
pub struct TranscriptRun {
    pub header: SessionLine,
    pub turns: Vec<TurnLine>,
}

/// Keeps every run for `--all`; otherwise keeps only the last run in ledger order.
pub fn select_runs(mut runs: Vec<TranscriptRun>, all: bool) -> Vec<TranscriptRun> {
    if all {
        return runs;
    }

    runs.pop().into_iter().collect()
}

/// Renders interactive warnings for source-integrity failures already carried by session records.
pub fn render_source_warnings(runs: &[TranscriptRun]) -> Vec<String> {
    let mut warnings = Vec::new();

    for run in runs {
        let source = &run.header.source;
        if source.complete {
            continue;
        }
        warnings.push(format!(
            "warning: run {} was read incompletely: {} skipped line(s), {} orphaned trace(s). See \
             `source` in the emitted session record.",
            run.header.run_index,
            source.skipped_line_count,
            source.orphans.total()
        ));
        warnings.extend(source.skipped_lines.iter().map(|skipped| {
            format!(
                "  line {}: kind {:?}: {}",
                skipped.line_number, skipped.kind, skipped.error
            )
        }));
    }

    warnings
}

/// One ledger line, in file order. Skipped lines stay in the sequence so they can be attributed to
/// the run — and where the envelope decoded an exchange index, to the turn — they belong to.
#[derive(Debug)]
pub enum LedgerEntry {
    Record(Box<DiagnosticRecord>),
    Skipped(SkippedLineView),
}

/// Everything gathered for one exchange index before it becomes a `TurnLine`.
#[derive(Default)]
struct TurnParts {
    exchange: Option<Exchange>,
    injection: Option<VolitionContextInjectionTrace>,
    formation: Option<(
        FormationStatus,
        Option<LiveGoalFormationTrace>,
        Option<String>,
    )>,
    initiative: Option<RealtimeBoundedInitiativeTrace>,
    world: Option<WorldConsultationTrace>,
    turn_context: Option<TurnContextView>,
    /// Kinds present for this index that could not be decoded.
    undecodable: Vec<String>,
}

struct RunParts {
    session_id: String,
    started_at: Option<OffsetDateTime>,
    turns: BTreeMap<usize, TurnParts>,
    /// Lines skipped inside this run, including those that named no exchange index.
    skipped: Vec<SkippedLineView>,
}

impl RunParts {
    fn new(session_id: String, started_at: Option<OffsetDateTime>) -> Self {
        Self {
            session_id,
            started_at,
            turns: BTreeMap::new(),
            skipped: Vec::new(),
        }
    }
}

pub fn runs_from_entries(
    entries: Vec<LedgerEntry>,
    ledger: &str,
    full: bool,
) -> Vec<TranscriptRun> {
    let mut runs: Vec<RunParts> = Vec::new();
    // Skipped lines seen before any run opened. Attached to the first run that does, rather than
    // inventing a phantom run for them.
    let mut pending_skipped: Vec<SkippedLineView> = Vec::new();

    for entry in entries {
        let record = match entry {
            LedgerEntry::Record(record) => *record,
            LedgerEntry::Skipped(skipped) => {
                match runs.last_mut() {
                    Some(run) => attribute_skipped(run, skipped),
                    None => pending_skipped.push(skipped),
                }
                continue;
            }
        };

        if let DiagnosticRecord::SessionAllocated { qsf_session_id, at } = &record {
            open_run(
                &mut runs,
                &mut pending_skipped,
                qsf_session_id.clone(),
                Some(*at),
            );
            continue;
        }

        if runs.is_empty() {
            open_run(
                &mut runs,
                &mut pending_skipped,
                session_id_of(&record),
                None,
            );
        }
        let run = runs.last_mut().expect("a run is open");

        match record {
            DiagnosticRecord::DiagnosticExchangeRecorded {
                trust: DiagnosticTrust::Trusted,
                exchange,
                ..
            } => {
                let index = exchange.index;
                run.turns.entry(index).or_default().exchange = Some(exchange);
            }
            DiagnosticRecord::DiagnosticExchangeRecorded { .. } => {}
            DiagnosticRecord::VolitionContextInjected {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().injection = Some(trace);
            }
            DiagnosticRecord::RealtimeBoundedInitiative {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().initiative = Some(trace);
            }
            DiagnosticRecord::WorldConsultationPerformed {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().world = Some(trace);
            }
            DiagnosticRecord::LiveGoalFormationPerformed {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().formation =
                    Some((FormationStatus::Performed, Some(trace), None));
            }
            DiagnosticRecord::LiveGoalFormationFailed {
                exchange_index,
                error,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().formation =
                    Some((FormationStatus::Failed, None, Some(error)));
            }
            DiagnosticRecord::LiveGoalFormationSkipped {
                exchange_index,
                reason,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().formation =
                    Some((FormationStatus::Skipped, None, Some(reason)));
            }
            DiagnosticRecord::TurnContextCaptured {
                exchange_index,
                request_hash,
                messages,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().turn_context = Some(TurnContextView {
                    request_hash,
                    messages,
                });
            }
            _ => {}
        }
    }

    // A ledger consisting only of undecodable lines still owes the caller an artifact that says so.
    if runs.is_empty() && !pending_skipped.is_empty() {
        open_run(&mut runs, &mut pending_skipped, String::new(), None);
    }

    runs.into_iter()
        .enumerate()
        .map(|(index, run)| build_run(index + 1, run, ledger, full))
        .collect()
}

/// Records an undecodable line against a run, and against a specific turn when its envelope named
/// an exchange index and a kind.
fn attribute_skipped(run: &mut RunParts, skipped: SkippedLineView) {
    if let (Some(index), Some(kind)) = (skipped.exchange_index, skipped.kind.clone()) {
        run.turns.entry(index).or_default().undecodable.push(kind);
    }
    run.skipped.push(skipped);
}

fn open_run(
    runs: &mut Vec<RunParts>,
    pending_skipped: &mut Vec<SkippedLineView>,
    session_id: String,
    started_at: Option<OffsetDateTime>,
) {
    runs.push(RunParts::new(session_id, started_at));
    let run = runs.last_mut().expect("just pushed");
    for skipped in pending_skipped.drain(..) {
        attribute_skipped(run, skipped);
    }
}

fn session_id_of(record: &DiagnosticRecord) -> String {
    match record {
        DiagnosticRecord::DiagnosticExchangeRecorded { qsf_session_id, .. }
        | DiagnosticRecord::VolitionContextInjected { qsf_session_id, .. }
        | DiagnosticRecord::RealtimeBoundedInitiative { qsf_session_id, .. }
        | DiagnosticRecord::WorldConsultationPerformed { qsf_session_id, .. }
        | DiagnosticRecord::LiveGoalFormationPerformed { qsf_session_id, .. }
        | DiagnosticRecord::LiveGoalFormationFailed { qsf_session_id, .. }
        | DiagnosticRecord::LiveGoalFormationSkipped { qsf_session_id, .. }
        | DiagnosticRecord::TurnContextCaptured { qsf_session_id, .. } => qsf_session_id.clone(),
        _ => String::new(),
    }
}

fn build_run(run_index: usize, run: RunParts, ledger: &str, full: bool) -> TranscriptRun {
    let mut orphans = OrphanCounts::default();
    let mut turns = Vec::new();

    for (index, mut parts) in run.turns {
        let Some(exchange) = parts.exchange.take() else {
            if parts.injection.is_some() {
                orphans.injection += 1;
            }
            if parts.formation.is_some() {
                orphans.formation += 1;
            }
            if parts.initiative.is_some() {
                orphans.initiative += 1;
            }
            if parts.world.is_some() {
                orphans.world += 1;
            }
            if parts.turn_context.is_some() {
                orphans.turn_context += 1;
            }
            continue;
        };
        turns.push(build_turn(index, exchange, parts, full));
    }

    let complete = run.skipped.is_empty() && orphans.total() == 0;
    TranscriptRun {
        header: SessionLine {
            session_id: run.session_id,
            ledger: ledger.to_string(),
            run_index,
            run_started_at: run.started_at.and_then(|at| at.format(&Rfc3339).ok()),
            turn_count: turns.len(),
            source: SourceIntegrity {
                complete,
                skipped_line_count: run.skipped.len(),
                skipped_lines: run.skipped,
                orphans,
            },
        },
        turns,
    }
}

fn build_turn(index: usize, exchange: Exchange, parts: TurnParts, full: bool) -> TurnLine {
    let user = match &exchange.input {
        ExchangeInput::Text { text } => text.clone(),
        ExchangeInput::Voice {
            final_transcript, ..
        } => final_transcript.clone(),
    };
    let assistant = exchange.output.as_ref().map(|output| output.text.clone());
    let at = exchange.completed_at.and_then(rfc3339_from_system_time);
    let status = serde_json::to_value(exchange.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());

    let volition = parts.injection.as_ref().map(volition_view);
    let initiative = parts.initiative.as_ref().map(initiative_view);
    let formation = parts
        .formation
        .as_ref()
        .map(|(status, trace, detail)| formation_view(*status, trace.as_ref(), detail.as_deref()));
    let world = parts.world.as_ref().map(world_view);
    let undecodable = parts.undecodable.clone();

    let traces = full.then(|| TraceBundle {
        injection: parts.injection,
        formation: parts.formation.and_then(|(_, trace, _)| trace),
        initiative: parts.initiative,
        world: parts.world,
        turn_context: parts.turn_context,
        exchange,
    });

    TurnLine {
        turn: index,
        at,
        user,
        assistant,
        status,
        volition,
        initiative,
        formation,
        world,
        undecodable,
        traces,
    }
}

fn rfc3339_from_system_time(value: SystemTime) -> Option<String> {
    OffsetDateTime::from(value).format(&Rfc3339).ok()
}

/// Splits the selector's matched goals at the qualification threshold, so `fired` means "could
/// win arbitration" and `below_threshold` means "matched but stayed quiet". A trace written before
/// `qualification_threshold` existed deserializes it as 0, in which case every match counts as
/// fired — the honest reading, since the threshold that applied is not recorded.
fn volition_view(trace: &VolitionContextInjectionTrace) -> VolitionView {
    let threshold = trace.qualification_threshold;
    let mut fired = Vec::new();
    let mut below_threshold = Vec::new();

    for detail in &trace.selector_output.selected_match_details {
        let reason = trace
            .below_threshold_candidates
            .iter()
            .find(|candidate| candidate.goal_id == detail.goal_id)
            .map(|candidate| candidate.reason.clone());
        let view = MatchView {
            goal: detail.goal_id.clone(),
            strength: detail.match_strength,
            keywords: detail.matched_keywords.iter().map(render_keyword).collect(),
            visibility: detail.visibility,
            reason,
        };
        if detail.match_strength >= threshold {
            fired.push(view);
        } else {
            below_threshold.push(view);
        }
    }

    VolitionView {
        threshold,
        mode: trace.arbitration_result.as_ref().map(|result| result.mode),
        winner: trace.arbitration_result.as_ref().map(|result| WinnerView {
            goal: result.winner_goal_id.clone(),
            title: result.winner_goal_title.clone(),
            effective_tier: result.winner_effective_tier,
            biased_tier: result.winner_biased_tier,
            losers: result.loser_count,
        }),
        fired,
        below_threshold,
        omitted_count: trace.selector_output.omitted_count,
        suppressed_cooldown_count: trace.selector_output.suppressed_cooldown_count,
        blocked_count: trace.selector_output.visible_blocked_count,
        subconscious_selected_count: trace.subconscious_selected_count,
    }
}

fn initiative_view(trace: &RealtimeBoundedInitiativeTrace) -> InitiativeView {
    InitiativeView {
        goal: trace.winning_goal_id.clone(),
        effect: trace.allowed_effect,
        surfaced: trace.surfaced,
        suppression: trace.suppression_reason,
        rendered_line_present: trace.rendered_line_present,
        output: trace.initiative_output.clone(),
    }
}

fn formation_view(
    status: FormationStatus,
    trace: Option<&LiveGoalFormationTrace>,
    detail: Option<&str>,
) -> FormationView {
    FormationView {
        status,
        candidate_id: trace.and_then(|t| t.proposed_candidate_id.clone()),
        candidate_title: trace.and_then(|t| t.proposed_candidate_title.clone()),
        contradictions: trace.map(|t| t.contradictions.clone()).unwrap_or_default(),
        resolution: trace.and_then(|t| t.resolution.clone()),
        declined: trace.and_then(|t| t.declined_candidate.clone()),
        detail: detail.map(str::to_string),
    }
}

fn world_view(trace: &WorldConsultationTrace) -> WorldView {
    WorldView {
        serving_goal: trace.serving_goal_id.clone(),
        serving_goal_title: trace.serving_goal_title.clone(),
        query_terms: trace.query_terms.clone(),
        surfaced_facts: trace
            .surfaced_facts
            .iter()
            .map(|fact| SurfacedFactView {
                title: fact.title.clone(),
                url: fact.url.clone(),
                source_domain: fact.source_domain.clone(),
                trust_tier: fact.trust_tier.clone(),
            })
            .collect(),
        injection_reason: trace.injection_reason.clone(),
        injected_chars: trace.injected_text.chars().count(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use qsf_diagnostics::{
        AmbientExposure, DiagnosticTrust, VolitionSelectedMatchDetail, VolitionSelectorSummary,
    };
    use qsf_session::{Exchange, ExchangeInput, ExchangeOutput, ExchangeStatus};
    use qsf_volition::{GoalVisibility, ShapingIntensity};

    use super::*;

    /// Test shim: the production entry point takes `LedgerEntry` values so an undecodable line keeps
    /// its place in file order. Most of these tests are about joining records, so they wrap plain
    /// records and keep the older, narrower call shape.
    fn runs_from_records(
        records: Vec<DiagnosticRecord>,
        ledger: &str,
        full: bool,
    ) -> Vec<TranscriptRun> {
        let entries = records
            .into_iter()
            .map(|record| LedgerEntry::Record(Box::new(record)))
            .collect();
        runs_from_entries(entries, ledger, full)
    }

    fn skipped(
        line_number: usize,
        kind: Option<&str>,
        exchange_index: Option<usize>,
    ) -> LedgerEntry {
        LedgerEntry::Skipped(SkippedLineView {
            line_number,
            kind: kind.map(str::to_string),
            exchange_index,
            error: "unknown variant".to_string(),
        })
    }

    fn trusted_exchange(index: usize, user: &str, assistant: &str) -> DiagnosticRecord {
        let started = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        DiagnosticRecord::DiagnosticExchangeRecorded {
            qsf_session_id: "s".to_string(),
            source: "sideband".to_string(),
            trust: DiagnosticTrust::Trusted,
            recorded_at: OffsetDateTime::UNIX_EPOCH,
            exchange: Exchange {
                index,
                started_at: started,
                completed_at: Some(started + Duration::from_secs(2)),
                input: ExchangeInput::Voice {
                    final_transcript: user.to_string(),
                    utterances: vec![],
                },
                output: Some(ExchangeOutput {
                    response_id: None,
                    text: assistant.to_string(),
                    produced_at: started + Duration::from_secs(2),
                    provider_name: None,
                    target: None,
                    audio_marker: None,
                }),
                context_assembly: None,
                retrieved_memory_block: String::new(),
                recalled_items: vec![],
                model: None,
                interruptions: vec![],
                provider_events: vec![],
                tool_requests: vec![],
                tool_executions: vec![],
                status: ExchangeStatus::Completed,
            },
        }
    }

    fn untrusted_exchange(index: usize, user: &str) -> DiagnosticRecord {
        let DiagnosticRecord::DiagnosticExchangeRecorded { exchange, .. } =
            trusted_exchange(index, user, "")
        else {
            unreachable!("constructor returns an exchange record")
        };
        DiagnosticRecord::DiagnosticExchangeRecorded {
            qsf_session_id: "s".to_string(),
            source: "browser_relay".to_string(),
            trust: DiagnosticTrust::Untrusted,
            recorded_at: OffsetDateTime::UNIX_EPOCH,
            exchange,
        }
    }

    fn allocated(session: &str) -> DiagnosticRecord {
        DiagnosticRecord::SessionAllocated {
            qsf_session_id: session.to_string(),
            at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn untrusted_relay_records_never_become_turns() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                untrusted_exchange(0, "relay observation"),
                trusted_exchange(0, "hello", "hi"),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].turns.len(), 1);
        assert_eq!(runs[0].turns[0].user, "hello");
        assert_eq!(runs[0].turns[0].assistant.as_deref(), Some("hi"));
    }

    #[test]
    fn each_session_allocation_starts_a_new_run() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "first run", "a"),
                allocated("s"),
                trusted_exchange(0, "second run", "b"),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].header.run_index, 1);
        assert_eq!(runs[1].header.run_index, 2);
        assert_eq!(runs[1].turns[0].user, "second run");
        assert_eq!(runs[0].header.turn_count, 1);
    }

    #[test]
    fn default_selection_keeps_the_last_run() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "first run", "a"),
                allocated("s"),
                trusted_exchange(0, "second run", "b"),
            ],
            "ledger.jsonl",
            false,
        );

        let selected = select_runs(runs, false);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].header.run_index, 2);
        assert_eq!(selected[0].turns[0].user, "second run");
    }

    #[test]
    fn all_selection_keeps_every_run_in_ledger_order() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "first run", "a"),
                allocated("s"),
                trusted_exchange(0, "second run", "b"),
            ],
            "ledger.jsonl",
            false,
        );

        let selected = select_runs(runs, true);

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].header.run_index, 1);
        assert_eq!(selected[1].header.run_index, 2);
    }

    #[test]
    fn source_warnings_render_run_and_skipped_line_details() {
        let runs = runs_from_entries(
            vec![
                LedgerEntry::Record(Box::new(allocated("s"))),
                LedgerEntry::Record(Box::new(trusted_exchange(0, "hello", "hi"))),
                skipped(7, Some("from_a_future_build"), Some(0)),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(
            render_source_warnings(&runs),
            vec![
                "warning: run 1 was read incompletely: 1 skipped line(s), 0 orphaned trace(s). See \
                 `source` in the emitted session record."
                    .to_string(),
                "  line 7: kind Some(\"from_a_future_build\"): unknown variant".to_string(),
            ]
        );
    }

    #[test]
    fn complete_runs_render_no_source_warnings() {
        let runs = runs_from_records(
            vec![allocated("s"), trusted_exchange(0, "hello", "hi")],
            "ledger.jsonl",
            false,
        );

        assert!(render_source_warnings(&runs).is_empty());
    }

    #[test]
    fn a_trace_with_no_matching_exchange_is_counted_as_an_orphan() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                DiagnosticRecord::LiveGoalFormationSkipped {
                    qsf_session_id: "s".to_string(),
                    exchange_index: 7,
                    recorded_at: OffsetDateTime::UNIX_EPOCH,
                    reason: "guard".to_string(),
                },
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs[0].turns.len(), 0);
        assert_eq!(runs[0].header.source.orphans.formation, 1);
        assert!(
            !runs[0].header.source.complete,
            "an orphaned trace makes the run incomplete"
        );
    }

    #[test]
    fn an_undecodable_line_marks_its_turn_rather_than_vanishing() {
        let runs = runs_from_entries(
            vec![
                LedgerEntry::Record(Box::new(allocated("s"))),
                LedgerEntry::Record(Box::new(trusted_exchange(0, "hello", "hi"))),
                skipped(7, Some("volition_context_injected"), Some(0)),
            ],
            "ledger.jsonl",
            false,
        );

        let turn = &runs[0].turns[0];
        assert!(turn.volition.is_none());
        assert_eq!(turn.undecodable, vec!["volition_context_injected"]);

        let source = &runs[0].header.source;
        assert!(!source.complete);
        assert_eq!(source.skipped_line_count, 1);
        assert_eq!(source.skipped_lines[0].line_number, 7);
    }

    #[test]
    fn an_undecodable_line_naming_no_turn_still_reaches_the_run_header() {
        let runs = runs_from_entries(
            vec![
                LedgerEntry::Record(Box::new(allocated("s"))),
                skipped(3, None, None),
                LedgerEntry::Record(Box::new(trusted_exchange(0, "hello", "hi"))),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs[0].turns[0].undecodable, Vec::<String>::new());
        assert_eq!(runs[0].header.source.skipped_line_count, 1);
        assert!(!runs[0].header.source.complete);
    }

    #[test]
    fn an_undecodable_line_before_the_first_run_joins_that_run_instead_of_making_one() {
        let runs = runs_from_entries(
            vec![
                skipped(1, Some("from_a_future_build"), None),
                LedgerEntry::Record(Box::new(allocated("s"))),
                LedgerEntry::Record(Box::new(trusted_exchange(0, "hello", "hi"))),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 1, "no phantom run for a leading skipped line");
        assert_eq!(runs[0].header.source.skipped_line_count, 1);
    }

    #[test]
    fn a_fully_read_run_is_marked_complete() {
        let runs = runs_from_records(
            vec![allocated("s"), trusted_exchange(0, "hello", "hi")],
            "ledger.jsonl",
            false,
        );

        assert!(runs[0].header.source.complete);
        assert_eq!(runs[0].header.source.skipped_line_count, 0);
        assert_eq!(runs[0].header.source.orphans.total(), 0);
    }

    #[test]
    fn turns_are_ordered_by_exchange_index() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(2, "third", "c"),
                trusted_exchange(0, "first", "a"),
                trusted_exchange(1, "second", "b"),
            ],
            "ledger.jsonl",
            false,
        );

        let users: Vec<&str> = runs[0].turns.iter().map(|t| t.user.as_str()).collect();
        assert_eq!(users, vec!["first", "second", "third"]);
    }

    #[test]
    fn records_before_any_allocation_form_a_run_without_a_start_time() {
        let runs = runs_from_records(
            vec![trusted_exchange(0, "orphaned run", "a")],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].header.run_started_at, None);
        assert_eq!(runs[0].turns.len(), 1);
    }

    #[test]
    fn a_repeated_exchange_index_keeps_the_last_record() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "first attempt", "partial"),
                trusted_exchange(0, "first attempt", "final answer"),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs[0].turns.len(), 1);
        assert_eq!(runs[0].turns[0].assistant.as_deref(), Some("final answer"));
    }

    fn injection_trace_with(
        threshold: u32,
        goals: Vec<(&str, u32)>,
    ) -> VolitionContextInjectionTrace {
        let selected_match_details = goals
            .into_iter()
            .map(|(goal_id, match_strength)| VolitionSelectedMatchDetail {
                goal_id: goal_id.to_string(),
                matched_keywords: vec![],
                match_strength,
                visibility: GoalVisibility::Conscious,
            })
            .collect();

        VolitionContextInjectionTrace {
            qsf_session_id: "s".to_string(),
            exchange_index: 0,
            injected_layers: vec![],
            stable_baseline_hash: String::new(),
            input_transcript_ref: String::new(),
            volition_tick_before: 0,
            events_applied: vec![],
            opportunity_signals: vec![],
            selector_output: VolitionSelectorSummary {
                selected_goal_ids: vec![],
                selected_goal_titles: vec![],
                selected_goal_summaries: vec![],
                selected_count: 0,
                omitted_count: 0,
                suppressed_cooldown_count: 0,
                visible_blocked_count: 0,
                selected_match_details,
            },
            omitted_or_suppressed_candidates: vec![],
            qualification_threshold: threshold,
            below_threshold_candidates: vec![],
            arbitration_result: None,
            mode_bias_outcomes: vec![],
            protected_tier_active: false,
            shaping_intensity: ShapingIntensity::None,
            shaping_intensity_inputs: None,
            context_packet_hash: String::new(),
            context_packet_token_estimate: 0,
            response_create_event_ref: String::new(),
            declined_candidates_injected: vec![],
            winner_visibility: None,
            ambient_exposure: AmbientExposure::Ordinary,
            subconscious_selected_count: 0,
        }
    }

    #[test]
    fn matches_split_on_the_qualification_threshold() {
        let trace = injection_trace_with(
            4,
            vec![("grow-the-library", 9), ("serve-the-present-person", 3)],
        );
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "do you remember", "yes"),
                DiagnosticRecord::VolitionContextInjected {
                    qsf_session_id: "s".to_string(),
                    exchange_index: 0,
                    recorded_at: OffsetDateTime::UNIX_EPOCH,
                    trace,
                },
            ],
            "ledger.jsonl",
            false,
        );

        let volition = runs[0].turns[0]
            .volition
            .as_ref()
            .expect("injection trace present");
        assert_eq!(volition.threshold, 4);
        assert_eq!(volition.fired.len(), 1);
        assert_eq!(volition.fired[0].goal, "grow-the-library");
        assert_eq!(volition.below_threshold.len(), 1);
        assert_eq!(volition.below_threshold[0].goal, "serve-the-present-person");
    }
}
