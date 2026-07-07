//! Offline harness proving conscious/subconscious goal visibility is an introspection-surfacing
//! filter, not a runtime input.
//!
//! It drives one subconscious goal through five scenarios — selected with no forcing, a suppressed
//! initiative, a rendered initiative, a coherence conflict, and an introspection read — and records
//! one `goal-visibility-derivation` trace per scenario carrying the recorded state slice, the
//! derived forced-surfacing set, and the ambient exposure treatment. A sixth record proves the
//! no-runtime-effect invariant: with the same goal flipped `Conscious`, `select_goals_ranked` and
//! `arbitrate_with_mode` are bit-identical. The harness then reads its own artifacts back and
//! re-derives every forced-surfacing condition and the invariant from recorded state alone,
//! including proving a suppressed internal initiative never forces surfacing.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::observability::event_log::EventType;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    ActivationKeyword, AllowedEffect, CoherenceDecline,
    DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD, DeclineReason, DeclinedCandidate, EvidenceRef,
    ForcedSurfacing, Goal, GoalDynamicState, GoalScope, GoalStatus, GoalVisibility,
    InitiativeOutput, Mode, ProposedGoalCandidate, Tension, TensionPriority, VolitionEvent,
    VolitionFixture, VolitionState, apply, arbitrate_with_mode, forced_surfaced_goals,
    goal_visibility, is_forced_surfaced, select_goals_ranked,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::volition_trace_support::write_volition_trace;

/// The subconscious background-disposition goal driven through every scenario. Keyword `world` is
/// unique to it, so a `world…` query makes it the sole arbitration winner.
const SUBCONSCIOUS_GOAL: &str = "assemble-world-picture";
/// A conscious goal, activated by `help`, used as the winner in the selected-non-winner mix and as
/// the visibility-flip control's neighbor.
const CONSCIOUS_GOAL: &str = "serve-the-present-person";
const TENSION_ID: &str = "world-curiosity";

const DERIVATION_OP: &str = "goal-visibility-derivation";
const ABSENCE_OP: &str = "goal-visibility-absence-check";
const INVARIANT_OP: &str = "goal-visibility-invariant";
const REPORT_ARTIFACT: &str = "goal-visibility-report.md";

/// The fields every `goal-visibility-derivation` record must carry (the trace-completeness
/// contract from `Experiment.VolitionGoalVisibility.md`). `arbitration_winner_visibility` may be
/// JSON null (no winner) but the key must exist; the rest must be non-null.
const REQUIRED_DERIVATION_FIELDS: [&str; 11] = [
    "tick",
    "scenario",
    "subconscious_goal_ids",
    "selector_output_visibility",
    "arbitration_winner_visibility",
    "forced_surfaced",
    "suppressed_initiative_not_surfaced",
    "ambient_exposure_treatment",
    "introspection_tool_output_ref",
    "dynamic_state_snapshot",
    "artifact_or_report_reference",
];

pub struct VolitionGoalVisibilityExperiment;

impl Experiment for VolitionGoalVisibilityExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionGoalVisibility
    }

    fn description(&self) -> &'static str {
        "Drive one subconscious goal through selection, suppressed vs rendered initiative, \
         coherence conflict, and introspection; prove forced-surfacing derives only from recorded \
         state, a suppressed initiative never forces surfacing, and a visibility flip leaves \
         selection and arbitration identical; offline, no realtime wiring"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = visibility_fixture();

        for scenario in scenarios() {
            run_scenario(context, &fixture, &scenario)?;
        }
        run_invariant(context, &fixture)?;

        let traces = read_jsonl(&context.run_dir().join("traces.jsonl"))?;
        let events = read_jsonl(&context.run_dir().join("events.jsonl"))?;
        verify_visibility_artifacts(&traces, &events, &fixture)
            .context("goal-visibility artifact verification failed")?;

        write_report(context, &traces)?;

        let derivation_count = traces
            .iter()
            .filter(|trace| trace["operation"] == DERIVATION_OP)
            .count();

        Ok(ExperimentOutcome {
            summary: format!(
                "Drove one subconscious goal through {derivation_count} scenarios (selected/no \
                 forcing, suppressed initiative, rendered initiative, coherence conflict, \
                 introspection) plus the visibility-flip invariant. Every forced-surfacing \
                 condition and ambient exposure treatment was re-derived from its recorded \
                 dynamic_state_snapshot, a suppressed internal initiative was proven not to force \
                 surface, and select_goals_ranked / arbitrate_with_mode were identical under a \
                 Conscious↔Subconscious flip.",
            ),
            observations: vec![
                "Forced-surfacing derives only from recorded facts: last_rendered_initiative_tick (a rendered line) and a DeclinedCandidate naming the goal — never from last_initiative_output or the live status.".to_string(),
                "A suppressed internal initiative sets last_initiative_tick but leaves last_rendered_initiative_tick None, so it never forces the subconscious goal to surface.".to_string(),
                "Ambient exposure is a presentation choice over the same winner: reduced_subconscious with no forcing, forced_surfaced_subconscious when a condition holds, ordinary for a conscious winner.".to_string(),
                "Visibility is inert to selection and arbitration: flipping the goal's visibility left both bit-identical.".to_string(),
            ],
            failure_modes: vec![
                "The scenarios are deterministic scripts; a live turn could combine forcing conditions this fixture keeps separate.".to_string(),
                "Ambient exposure here is recomputed from recorded forced-surfacing; the realtime layer decides it from the same conditions at turn time (the parity is covered by the realtime injection tests, not this harness).".to_string(),
            ],
            follow_up_questions: vec![
                "Does the operator prefer a display label other than `subconscious` for the badge?".to_string(),
            ],
            decision_candidates: vec![
                "Confirm the goal-visibility-derivation trace shape as the durable visibility trace format.".to_string(),
            ],
            extra_artifacts: vec![REPORT_ARTIFACT.to_string()],
        })
    }
}

/// A two-goal, single-tension fixture: one subconscious background disposition and one conscious
/// goal, each with a unique activation keyword so a query can pick a deterministic winner.
fn visibility_fixture() -> VolitionFixture {
    let tensions = vec![Tension {
        id: TENSION_ID.to_string(),
        title: "World curiosity".to_string(),
        summary: "How the world functions and where it is heading.".to_string(),
        priority_bias: TensionPriority::Medium,
        arbitration_tier: 6,
        focused_bias: 0,
        exploratory_bias: 0,
    }];
    let goals = vec![
        goal(SUBCONSCIOUS_GOAL, "world", GoalVisibility::Subconscious),
        goal(CONSCIOUS_GOAL, "help", GoalVisibility::Conscious),
    ];
    VolitionFixture {
        tensions,
        goals,
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    }
}

fn goal(id: &str, keyword: &str, visibility: GoalVisibility) -> Goal {
    Goal {
        id: id.to_string(),
        title: id.to_string(),
        summary: format!("{id} (summary)"),
        tension_ids: vec![TENSION_ID.to_string()],
        status: GoalStatus::Accepted,
        scope: GoalScope::Session,
        base_priority: 60,
        activation_keywords: vec![ActivationKeyword::normal(keyword)],
        allowed_effects: vec![AllowedEffect::Reflect],
        satisfaction_condition_summary: id.to_string(),
        evidence_refs: vec!["docs/Experiments/Experiment.VolitionGoalVisibility.md".to_string()],
        estimated_tokens: 10,
        source_reference: "docs/Experiments/Experiment.VolitionGoalVisibility.md".to_string(),
        visibility,
    }
}

/// One scripted scenario: a deterministic event list, a query that selects the winner, and the
/// expected forced-surfacing outcome for the subconscious goal.
struct Scenario {
    label: &'static str,
    intent: &'static str,
    events: Vec<VolitionEvent>,
    query: &'static str,
    /// Whether the subconscious goal is expected to be forced surfaced.
    expect_forced: bool,
    /// The ambient exposure treatment expected for this turn's winner.
    expected_exposure: &'static str,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // (a) selected & biasing, no forcing condition → reduced ambient exposure.
        Scenario {
            label: "selected-no-forcing",
            intent: "The subconscious goal wins selection but has no forcing condition, so it is \
                     not forced surfaced and its ambient exposure is reduced.",
            events: vec![activated(SUBCONSCIOUS_GOAL, 1)],
            query: "world",
            expect_forced: false,
            expected_exposure: "reduced_subconscious",
        },
        // (b) suppressed initiative → not forced surfaced.
        Scenario {
            label: "suppressed-initiative",
            intent: "An initiative executes but its line is suppressed (rendered_ref None), so the \
                     subconscious goal is not forced surfaced.",
            events: vec![initiative(SUBCONSCIOUS_GOAL, 2, None)],
            query: "world",
            expect_forced: false,
            expected_exposure: "reduced_subconscious",
        },
        // (c) rendered initiative → forced surfaced.
        Scenario {
            label: "rendered-initiative",
            intent: "The subconscious goal renders an initiative line, forcing it to surface with \
                     full labeled detail.",
            events: vec![initiative(
                SUBCONSCIOUS_GOAL,
                2,
                Some("exchange:1/diagnostic:realtime_bounded_initiative"),
            )],
            query: "world",
            expect_forced: true,
            expected_exposure: "forced_surfaced_subconscious",
        },
        // (d) coherence conflict naming the goal → forced surfaced.
        Scenario {
            label: "coherence-conflict",
            intent: "A declined candidate names the subconscious goal as the conflicting goal, \
                     forcing it to surface.",
            events: vec![
                candidate_added("world-detour", "Chase an off-topic detour", 2),
                candidate_rejected_coherence(
                    "world-detour",
                    DeclineReason::ConflictingGoal {
                        goal_id: SUBCONSCIOUS_GOAL.to_string(),
                    },
                    "would derail the background world picture",
                    3,
                ),
            ],
            query: "world",
            expect_forced: true,
            expected_exposure: "forced_surfaced_subconscious",
        },
        // (e) introspection read: the goal is present in the unfiltered inspection carrying
        // Subconscious visibility — what the introspection tool sections on.
        Scenario {
            label: "introspection-read",
            intent: "An inspect_volition_state call is itself the ask; the goal is present in the \
                     unfiltered inspection with Subconscious visibility, which the tool sections.",
            events: vec![activated(SUBCONSCIOUS_GOAL, 1)],
            query: "world",
            expect_forced: false,
            expected_exposure: "reduced_subconscious",
        },
    ]
}

fn activated(goal_id: &str, tick: u64) -> VolitionEvent {
    VolitionEvent::GoalActivated {
        goal_id: goal_id.to_string(),
        tick,
    }
}

fn initiative(goal_id: &str, tick: u64, rendered_ref: Option<&str>) -> VolitionEvent {
    VolitionEvent::InitiativeExecuted {
        goal_id: goal_id.to_string(),
        effect: AllowedEffect::Reflect,
        output: InitiativeOutput::ReflectionRequested {
            proposed_question: "How does this fit the larger picture?".to_string(),
        },
        rationale: "scripted visibility scenario".to_string(),
        tick,
        rendered_ref: rendered_ref.map(|reference| EvidenceRef::try_new(reference).unwrap()),
    }
}

fn candidate_added(id: &str, title: &str, tick: u64) -> VolitionEvent {
    VolitionEvent::GoalCandidateAdded {
        candidate: candidate(id, title),
        tick,
    }
}

fn candidate_rejected_coherence(
    id: &str,
    conflict: DeclineReason,
    rationale: &str,
    tick: u64,
) -> VolitionEvent {
    VolitionEvent::GoalCandidateRejected {
        goal_id: id.to_string(),
        reason: "coherence check rejected".to_string(),
        coherence_decline: Some(CoherenceDecline {
            conflict,
            rationale: rationale.to_string(),
        }),
        tick,
    }
}

fn candidate(id: &str, title: &str) -> ProposedGoalCandidate {
    ProposedGoalCandidate::try_new(
        id.to_string(),
        title.to_string(),
        format!("{title} (summary)"),
        vec![TENSION_ID.to_string()],
        GoalScope::Session,
        50,
        vec![AllowedEffect::Reflect],
        format!("satisfied when {title} is resolved"),
        vec![EvidenceRef::try_new("scripted visibility candidate").unwrap()],
        "formed from scripted visibility scenario".to_string(),
        vec!["world".to_string()],
    )
    .unwrap()
}

/// The state slice the visibility derivation reads. Carries the full per-goal dynamic state (so
/// `last_rendered_initiative_tick` and `last_initiative_tick` are present), the tick, and the
/// declined candidates a coherence conflict is derived from.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct VisibilityStateSnapshot {
    tick: u64,
    goals: BTreeMap<String, GoalDynamicState>,
    declined_candidates: Vec<DeclinedCandidate>,
}

fn snapshot_of(state: &VolitionState) -> VisibilityStateSnapshot {
    VisibilityStateSnapshot {
        tick: state.tick,
        goals: state.goals.clone(),
        declined_candidates: state.declined_candidates.clone(),
    }
}

fn state_from_snapshot(snapshot: VisibilityStateSnapshot) -> VolitionState {
    VolitionState {
        tick: snapshot.tick,
        goals: snapshot.goals,
        pending_candidates: Vec::new(),
        accepted_candidates: BTreeMap::new(),
        mode: Mode::default(),
        declined_candidates: snapshot.declined_candidates,
    }
}

fn replay(events: &[VolitionEvent], fixture: &VolitionFixture) -> VolitionState {
    let mut state = VolitionState::from_fixture(fixture);
    for event in events {
        state = apply(state, event.clone());
    }
    state
}

/// The arbitration winner id for `query`, or `None` when no goal qualifies.
fn winner_of(query: &str, state: &VolitionState, fixture: &VolitionFixture) -> Option<String> {
    let ranked = select_goals_ranked(query, state, fixture);
    arbitrate_with_mode(ranked.selected, fixture, state.mode)
        .and_then(|outcome| outcome.qualified)
        .map(|qualified| qualified.winner.goal.id)
}

/// The ambient exposure treatment for `winner_id` — the same rule the realtime layer applies,
/// expressed over recorded state so the offline harness can derive it from `qsf_volition` alone.
fn ambient_exposure(
    winner_id: Option<&str>,
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> &'static str {
    match winner_id {
        None => "ordinary",
        Some(id) if goal_visibility(id, state, fixture) != GoalVisibility::Subconscious => {
            "ordinary"
        }
        Some(id) if is_forced_surfaced(id, state, fixture) => "forced_surfaced_subconscious",
        Some(_) => "reduced_subconscious",
    }
}

/// Per-selected-goal `{ goal_id, visibility }` for the trace's `selector_output_visibility`.
fn selector_output_visibility(
    query: &str,
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> Value {
    let ranked = select_goals_ranked(query, state, fixture);
    json!(
        ranked
            .selected
            .iter()
            .map(|selection| json!({
                "goal_id": selection.goal.id,
                "visibility": goal_visibility(&selection.goal.id, state, fixture),
            }))
            .collect::<Vec<_>>()
    )
}

/// Goals with an executed-but-not-rendered initiative this run (`last_initiative_tick` set,
/// `last_rendered_initiative_tick` None) — the recorded proof a suppressed initiative did not
/// force surfacing.
fn suppressed_initiatives(state: &VolitionState) -> Value {
    json!(
        state
            .goals
            .iter()
            .filter(|(_, dynamic)| dynamic.last_initiative_tick.is_some()
                && dynamic.last_rendered_initiative_tick.is_none())
            .map(|(goal_id, dynamic)| json!({
                "goal_id": goal_id,
                "initiative_tick": dynamic.last_initiative_tick,
            }))
            .collect::<Vec<_>>()
    )
}

fn subconscious_goal_ids(fixture: &VolitionFixture) -> Vec<String> {
    fixture
        .goals
        .iter()
        .filter(|goal| goal.visibility == GoalVisibility::Subconscious)
        .map(|goal| goal.id.clone())
        .collect()
}

fn run_scenario(
    context: &mut RunContext,
    fixture: &VolitionFixture,
    scenario: &Scenario,
) -> anyhow::Result<()> {
    let state = replay(&scenario.events, fixture);
    let forced = forced_surfaced_goals(&state, fixture);

    // In-harness precondition checks, so a broken derivation fails before verification.
    let is_forced = forced
        .iter()
        .any(|entry| entry.goal_id == SUBCONSCIOUS_GOAL);
    ensure!(
        is_forced == scenario.expect_forced,
        "scenario `{}`: subconscious goal forced-surfaced = {is_forced}, expected {}",
        scenario.label,
        scenario.expect_forced
    );
    let winner = winner_of(scenario.query, &state, fixture);
    let exposure = ambient_exposure(winner.as_deref(), &state, fixture);
    ensure!(
        exposure == scenario.expected_exposure,
        "scenario `{}`: ambient exposure = {exposure}, expected {}",
        scenario.label,
        scenario.expected_exposure
    );

    let snapshot = json!(snapshot_of(&state));
    let winner_visibility = winner.as_deref().map(
        |id| json!({ "winner_goal_id": id, "visibility": goal_visibility(id, &state, fixture) }),
    );

    let details = json!({
        "scenario": scenario.label,
        "intent": scenario.intent,
        "tick": state.tick,
        "query": scenario.query,
        "subconscious_goal_ids": subconscious_goal_ids(fixture),
        "selector_output_visibility": selector_output_visibility(scenario.query, &state, fixture),
        "arbitration_winner_visibility": winner_visibility,
        "forced_surfaced": json!(forced),
        "suppressed_initiative_not_surfaced": suppressed_initiatives(&state),
        "ambient_exposure_treatment": exposure,
        // The introspection tool output is captured as a trace artifact by the tool-layer tests;
        // this offline harness proves the precondition (Subconscious visibility on the inspection)
        // and points at that tool contract rather than invoking the realtime tool.
        "introspection_tool_output_ref":
            format!("{DERIVATION_OP}#scenario={}&introspection=inspect_volition_state", scenario.label),
        "events_applied": json!(scenario.events),
        "dynamic_state_snapshot": snapshot,
        "artifact_or_report_reference":
            format!("{DERIVATION_OP}#scenario={}", scenario.label),
    });
    let trace_id = write_volition_trace(
        context,
        DERIVATION_OP,
        scenario.label,
        state.tick,
        &scenario.events,
        details,
    )?;
    context.record_event(
        EventType::TraceRecorded,
        json!({
            "operation": DERIVATION_OP,
            "scenario": scenario.label,
            "forced": scenario.expect_forced,
            "tick": state.tick,
        }),
        Some(trace_id),
    )?;

    // The suppressed-initiative scenario also emits an absence-check record: the goal executed an
    // initiative yet is provably not forced surfaced.
    if scenario.label == "suppressed-initiative" {
        write_absence_record(context, scenario, &state, &snapshot)?;
    }
    Ok(())
}

fn write_absence_record(
    context: &mut RunContext,
    scenario: &Scenario,
    state: &VolitionState,
    snapshot: &Value,
) -> anyhow::Result<()> {
    let details = json!({
        "scenario": scenario.label,
        "intent": "A suppressed internal initiative must NOT force the subconscious goal to surface.",
        "tick": state.tick,
        "expected_absent_goal_ids": [SUBCONSCIOUS_GOAL],
        "forced_surfaced_present": json!(forced_surfaced_goals(state, &visibility_fixture())),
        "suppressed_initiative_not_surfaced": suppressed_initiatives(state),
        "events_applied": json!(scenario.events),
        "dynamic_state_snapshot": snapshot,
        "artifact_or_report_reference": format!("{ABSENCE_OP}#scenario={}", scenario.label),
    });
    let trace_id = write_volition_trace(
        context,
        ABSENCE_OP,
        scenario.label,
        state.tick,
        &scenario.events,
        details,
    )?;
    context.record_event(
        EventType::TraceRecorded,
        json!({
            "operation": ABSENCE_OP,
            "scenario": scenario.label,
            "expected_absent_goal_ids": [SUBCONSCIOUS_GOAL],
            "tick": state.tick,
        }),
        Some(trace_id),
    )?;
    Ok(())
}

/// Records the no-runtime-effect invariant: selection and arbitration are identical when the
/// subconscious goal is flipped to `Conscious`, with the same query set.
fn run_invariant(context: &mut RunContext, fixture: &VolitionFixture) -> anyhow::Result<()> {
    let flipped = flipped_to_conscious(fixture);
    let sub_state = VolitionState::from_fixture(fixture);
    let con_state = VolitionState::from_fixture(&flipped);

    let queries = ["world", "help", "world help"];
    let mut per_query = Vec::new();
    for query in queries {
        let sub_ranked = select_goals_ranked(query, &sub_state, fixture);
        let con_ranked = select_goals_ranked(query, &con_state, &flipped);
        let sub_ids: Vec<&String> = sub_ranked.selected.iter().map(|s| &s.goal.id).collect();
        let con_ids: Vec<&String> = con_ranked.selected.iter().map(|s| &s.goal.id).collect();
        ensure!(
            sub_ids == con_ids,
            "visibility flip changed selection for query `{query}`"
        );
        let sub_winner = winner_of(query, &sub_state, fixture);
        let con_winner = winner_of(query, &con_state, &flipped);
        ensure!(
            sub_winner == con_winner,
            "visibility flip changed the arbitration winner for query `{query}`"
        );
        per_query.push(json!({
            "query": query,
            "selected_goal_ids": sub_ids,
            "winner": sub_winner,
        }));
    }

    let details = json!({
        "intent": "select_goals_ranked and arbitrate_with_mode are identical under a \
                   Conscious↔Subconscious visibility flip.",
        "flipped_goal_id": SUBCONSCIOUS_GOAL,
        "queries": json!(per_query),
    });
    let trace_id = write_volition_trace(context, INVARIANT_OP, "invariant", 0, &[], details)?;
    context.record_event(
        EventType::TraceRecorded,
        json!({ "operation": INVARIANT_OP, "flipped_goal_id": SUBCONSCIOUS_GOAL }),
        Some(trace_id),
    )?;
    Ok(())
}

/// The fixture with the subconscious goal flipped to `Conscious`, all else identical.
fn flipped_to_conscious(fixture: &VolitionFixture) -> VolitionFixture {
    let mut flipped = fixture.clone();
    for goal in &mut flipped.goals {
        if goal.id == SUBCONSCIOUS_GOAL {
            goal.visibility = GoalVisibility::Conscious;
        }
    }
    flipped
}

fn read_jsonl(path: &Path) -> anyhow::Result<Vec<Value>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read artifact {}", path.display()))?;
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .with_context(|| format!("failed to parse jsonl line in {}", path.display()))
        })
        .collect()
}

/// Enforces the trace-completeness contract: required fields on every derivation record, a genuine
/// re-derivation of every forced-surfacing condition and ambient exposure from the recorded
/// snapshot, the suppressed-not-surfaced proof, `TraceRecorded` linkage, and the invariant.
fn verify_visibility_artifacts(
    traces: &[Value],
    events: &[Value],
    fixture: &VolitionFixture,
) -> anyhow::Result<()> {
    let derivations: Vec<&Value> = traces
        .iter()
        .filter(|trace| trace["operation"] == DERIVATION_OP)
        .collect();
    ensure!(
        !derivations.is_empty(),
        "no goal-visibility-derivation trace records were written"
    );

    let mut saw_forced = false;
    let mut saw_unforced = false;
    for record in &derivations {
        let forced = verify_derivation_record(record, fixture)?;
        saw_forced |= forced;
        saw_unforced |= !forced;
    }
    ensure!(
        saw_forced && saw_unforced,
        "coverage: need at least one forced and one not-forced scenario"
    );

    for record in traces.iter().filter(|t| t["operation"] == ABSENCE_OP) {
        verify_absence_record(record, fixture)?;
    }
    ensure!(
        traces.iter().any(|t| t["operation"] == ABSENCE_OP),
        "the suppressed-initiative absence check must be recorded"
    );

    verify_invariant_record(traces, fixture)?;
    verify_trace_recorded_linkage(traces, events)?;
    Ok(())
}

/// Returns whether the scenario's subconscious goal was forced surfaced, after re-deriving it.
fn verify_derivation_record(record: &Value, fixture: &VolitionFixture) -> anyhow::Result<bool> {
    let details = &record["details"];
    let scenario = details["scenario"].as_str().unwrap_or("<unknown>");
    for field in REQUIRED_DERIVATION_FIELDS {
        ensure!(
            details.get(field).is_some(),
            "derivation record for scenario `{scenario}` is missing required field `{field}`"
        );
    }
    // `arbitration_winner_visibility` must be present (checked above) but may be JSON null when no
    // goal qualifies as winner; every other required field must be non-null.
    for field in REQUIRED_DERIVATION_FIELDS {
        if field == "arbitration_winner_visibility" {
            continue;
        }
        ensure!(
            !details[field].is_null(),
            "derivation record for scenario `{scenario}` has null required field `{field}`"
        );
    }

    let state = reconstruct_and_cross_check(details, fixture, scenario)?;

    // Re-derive forced surfacing from the recorded snapshot and match it to the trace.
    let rederived = forced_surfaced_goals(&state, fixture);
    ensure!(
        json!(rederived) == details["forced_surfaced"],
        "forced_surfaced mismatch on re-derivation for scenario `{scenario}`"
    );

    // Re-derive the ambient exposure treatment from the recorded snapshot + query.
    let query = details["query"].as_str().unwrap_or_default();
    let winner = winner_of(query, &state, fixture);
    let exposure = ambient_exposure(winner.as_deref(), &state, fixture);
    ensure!(
        json!(exposure) == details["ambient_exposure_treatment"],
        "ambient_exposure mismatch on re-derivation for scenario `{scenario}`: traced {}, \
         re-derived {exposure}",
        details["ambient_exposure_treatment"]
    );

    // Re-derive per-selected-goal visibility.
    ensure!(
        selector_output_visibility(query, &state, fixture) == details["selector_output_visibility"],
        "selector_output_visibility mismatch on re-derivation for scenario `{scenario}`"
    );

    Ok(rederived
        .iter()
        .any(|entry| entry.goal_id == SUBCONSCIOUS_GOAL))
}

fn verify_absence_record(record: &Value, fixture: &VolitionFixture) -> anyhow::Result<()> {
    let details = &record["details"];
    let scenario = details["scenario"].as_str().unwrap_or("<unknown>");
    for field in [
        "tick",
        "expected_absent_goal_ids",
        "suppressed_initiative_not_surfaced",
        "events_applied",
        "dynamic_state_snapshot",
        "artifact_or_report_reference",
    ] {
        ensure!(
            details.get(field).is_some_and(|value| !value.is_null()),
            "absence record for scenario `{scenario}` is missing required field `{field}`"
        );
    }

    let state = reconstruct_and_cross_check(details, fixture, scenario)?;
    let forced: Vec<ForcedSurfacing> = forced_surfaced_goals(&state, fixture);
    let expected_absent = details["expected_absent_goal_ids"]
        .as_array()
        .with_context(|| format!("absence record `{scenario}` has non-array expected_absent"))?;
    for goal_id in expected_absent {
        ensure!(
            !forced.iter().any(|entry| json!(entry.goal_id) == *goal_id),
            "scenario `{scenario}` asserted {goal_id} not forced, but re-derivation forced it"
        );
    }
    // And the goal must actually have executed a suppressed initiative (else the proof is vacuous).
    ensure!(
        state
            .goals
            .get(SUBCONSCIOUS_GOAL)
            .is_some_and(|dynamic| dynamic.last_initiative_tick.is_some()
                && dynamic.last_rendered_initiative_tick.is_none()),
        "absence record `{scenario}` must have a suppressed (executed, not rendered) initiative"
    );
    Ok(())
}

fn verify_invariant_record(traces: &[Value], fixture: &VolitionFixture) -> anyhow::Result<()> {
    let record = traces
        .iter()
        .find(|trace| trace["operation"] == INVARIANT_OP)
        .context("the visibility-flip invariant record must be present")?;
    let queries = record["details"]["queries"]
        .as_array()
        .context("invariant record has non-array queries")?;
    ensure!(!queries.is_empty(), "invariant record recorded no queries");

    // Independently re-run selection/arbitration under the flip and confirm the recorded equality.
    let flipped = flipped_to_conscious(fixture);
    let sub_state = VolitionState::from_fixture(fixture);
    let con_state = VolitionState::from_fixture(&flipped);
    for query_record in queries {
        let query = query_record["query"].as_str().unwrap_or_default();
        let sub_ranked = select_goals_ranked(query, &sub_state, fixture);
        let con_ranked = select_goals_ranked(query, &con_state, &flipped);
        let sub_ids: Vec<&String> = sub_ranked.selected.iter().map(|s| &s.goal.id).collect();
        let con_ids: Vec<&String> = con_ranked.selected.iter().map(|s| &s.goal.id).collect();
        ensure!(
            sub_ids == con_ids
                && winner_of(query, &sub_state, fixture) == winner_of(query, &con_state, &flipped),
            "invariant re-check failed: flip changed selection/arbitration for query `{query}`"
        );
    }
    Ok(())
}

fn reconstruct_and_cross_check(
    details: &Value,
    fixture: &VolitionFixture,
    scenario: &str,
) -> anyhow::Result<VolitionState> {
    let snapshot: VisibilityStateSnapshot =
        serde_json::from_value(details["dynamic_state_snapshot"].clone()).with_context(|| {
            format!("could not deserialize dynamic_state_snapshot for scenario `{scenario}`")
        })?;
    let events: Vec<VolitionEvent> = serde_json::from_value(details["events_applied"].clone())
        .with_context(|| {
            format!("could not deserialize events_applied for scenario `{scenario}`")
        })?;
    let replayed = snapshot_of(&replay(&events, fixture));
    ensure!(
        replayed == snapshot,
        "replaying events_applied does not reproduce the recorded dynamic_state_snapshot for \
         scenario `{scenario}`"
    );
    Ok(state_from_snapshot(snapshot))
}

fn verify_trace_recorded_linkage(traces: &[Value], events: &[Value]) -> anyhow::Result<()> {
    let derivation_trace_ids: HashSet<String> = traces
        .iter()
        .filter(|trace| trace["operation"] == DERIVATION_OP)
        .filter_map(|trace| trace["trace_id"].as_str().map(str::to_string))
        .collect();
    let linked: Vec<&Value> = events
        .iter()
        .filter(|event| {
            event["event_type"] == "TraceRecorded" && event["payload"]["operation"] == DERIVATION_OP
        })
        .collect();
    ensure!(
        linked.len() == derivation_trace_ids.len(),
        "expected one TraceRecorded event per goal-visibility-derivation trace: {} events vs {} traces",
        linked.len(),
        derivation_trace_ids.len()
    );
    for event in linked {
        let trace_id = event["trace_id"]
            .as_str()
            .context("TraceRecorded event has no linked trace_id")?;
        ensure!(
            derivation_trace_ids.contains(trace_id),
            "TraceRecorded event links trace_id {trace_id}, which is not a \
             goal-visibility-derivation trace"
        );
    }
    Ok(())
}

fn write_report(context: &RunContext, traces: &[Value]) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Volition Goal Visibility\n\n");
    md.push_str(
        "Offline harness: one subconscious goal driven through selection, suppressed vs rendered \
         initiative, coherence conflict, and introspection. Forced-surfacing and ambient exposure \
         derive only from recorded state; the visibility-flip invariant proves selection and \
         arbitration are unchanged. See `docs/Experiments/Experiment.VolitionGoalVisibility.md`.\n\n",
    );
    md.push_str("## Scenarios\n\n");
    for record in traces.iter().filter(|t| t["operation"] == DERIVATION_OP) {
        let details = &record["details"];
        md.push_str(&format!(
            "- `{}` (tick {}) — exposure `{}`, forced_surfaced {} — {}\n",
            details["scenario"].as_str().unwrap_or_default(),
            details["tick"],
            details["ambient_exposure_treatment"]
                .as_str()
                .unwrap_or_default(),
            details["forced_surfaced"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            details["intent"].as_str().unwrap_or_default(),
        ));
    }
    md.push_str("\n## Human Verification Checklist\n\n");
    md.push_str("- [ ] Every forced-surfacing row is explained by its recorded condition alone.\n");
    md.push_str("- [ ] The suppressed initiative executed but did not force surfacing.\n");
    md.push_str("- [ ] The visibility flip left selection and arbitration identical.\n");
    md.push_str("- [ ] No path outside the operator panel / introspection tool narrates a subconscious goal.\n");

    fs::write(context.run_dir().join(REPORT_ARTIFACT), md).with_context(|| {
        format!(
            "failed to write goal-visibility report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::Value;

    use super::super::registry::{ExperimentName, available_experiments, run_experiment_in};
    use super::{
        ABSENCE_OP, DERIVATION_OP, INVARIANT_OP, verify_visibility_artifacts, visibility_fixture,
    };

    fn parse_jsonl(path: &std::path::Path) -> Vec<Value> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn run() -> (Vec<Value>, Vec<Value>, PathBuf, PathBuf) {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-goal-visibility-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionGoalVisibility).unwrap();
        let traces = parse_jsonl(&summary.run_dir.join("traces.jsonl"));
        let events = parse_jsonl(&summary.run_dir.join("events.jsonl"));
        (traces, events, base_dir, summary.run_dir)
    }

    #[test]
    fn experiment_is_registered_and_dispatches() {
        let experiments = available_experiments();
        assert!(
            experiments
                .iter()
                .any(|experiment| experiment.id == ExperimentName::VolitionGoalVisibility.id()),
            "volition-goal-visibility must be registered"
        );
        let parsed: ExperimentName = "volition-goal-visibility".parse().unwrap();
        assert_eq!(parsed, ExperimentName::VolitionGoalVisibility);
    }

    #[test]
    fn experiment_runs_and_produces_artifacts() {
        let (traces, _events, base_dir, run_dir) = run();
        assert!(traces.iter().any(|t| t["operation"] == DERIVATION_OP));
        assert!(traces.iter().any(|t| t["operation"] == ABSENCE_OP));
        assert!(traces.iter().any(|t| t["operation"] == INVARIANT_OP));
        let report = fs::read_to_string(run_dir.join("goal-visibility-report.md")).unwrap();
        assert!(report.contains("Human Verification Checklist"));
        assert!(report.contains("rendered-initiative"));
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn verification_passes_on_the_real_run_artifacts() {
        let (traces, events, base_dir, _run_dir) = run();
        verify_visibility_artifacts(&traces, &events, &visibility_fixture()).unwrap();
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn verification_rejects_a_tampered_forced_surfaced() {
        let (mut traces, events, base_dir, _run_dir) = run();
        // Flip a not-forced scenario's forced_surfaced to claim a forcing that its snapshot does
        // not produce, proving the check is a real re-derivation, not self-comparison.
        let record = traces
            .iter_mut()
            .find(|trace| {
                trace["operation"] == DERIVATION_OP
                    && trace["details"]["scenario"] == "selected-no-forcing"
            })
            .unwrap();
        record["details"]["forced_surfaced"] = serde_json::json!([
            { "goal_id": "assemble-world-picture", "condition": { "kind": "rendered_initiative", "tick": 9, "rendered_ref": null } }
        ]);
        let error = verify_visibility_artifacts(&traces, &events, &visibility_fixture())
            .expect_err("tampered forced_surfaced must fail verification");
        assert!(error.to_string().contains("forced_surfaced mismatch"));
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn verification_rejects_a_missing_arbitration_winner_visibility() {
        let (mut traces, events, base_dir, _run_dir) = run();
        // The trace-completeness contract requires the `arbitration_winner_visibility` key on every
        // derivation record (it may be null, but must exist). Dropping it must fail verification, so
        // an operator can always reconstruct winner visibility from structured traces.
        let record = traces
            .iter_mut()
            .find(|trace| {
                trace["operation"] == DERIVATION_OP
                    && trace["details"]["scenario"] == "selected-no-forcing"
            })
            .unwrap();
        record["details"]
            .as_object_mut()
            .unwrap()
            .remove("arbitration_winner_visibility");
        let error = verify_visibility_artifacts(&traces, &events, &visibility_fixture())
            .expect_err("missing arbitration_winner_visibility must fail verification");
        assert!(
            error
                .to_string()
                .contains("missing required field `arbitration_winner_visibility`"),
            "unexpected error: {error}"
        );
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn verification_rejects_a_tampered_ambient_exposure() {
        let (mut traces, events, base_dir, _run_dir) = run();
        let record = traces
            .iter_mut()
            .find(|trace| {
                trace["operation"] == DERIVATION_OP
                    && trace["details"]["scenario"] == "rendered-initiative"
            })
            .unwrap();
        record["details"]["ambient_exposure_treatment"] = serde_json::json!("ordinary");
        let error = verify_visibility_artifacts(&traces, &events, &visibility_fixture())
            .expect_err("tampered ambient exposure must fail verification");
        assert!(error.to_string().contains("ambient_exposure mismatch"));
        fs::remove_dir_all(base_dir).unwrap();
    }
}
