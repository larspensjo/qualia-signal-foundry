//! Offline harness proving the four functional signals (`coherence_decline`, `frustration`,
//! `satisfaction`, `boredom`) derive *only* from recorded volition state, each with a parseable
//! trace per derivation.
//!
//! For every scripted scenario the harness builds a `VolitionState` by replaying a deterministic
//! list of lifecycle [`VolitionEvent`]s, derives signals with the pure
//! [`derive_signals`](crate::volition::derive_signals), and records one `emotion-signal-derivation`
//! trace per emitted signal plus one `emotion-signal-absence-check` trace per scenario. It then
//! reads its own `traces.jsonl` / `events.jsonl` back and re-derives every traced signal from the
//! recorded `dynamic_state_snapshot`, failing the run if any re-derivation disagrees. Signals are
//! only derived, recorded, and asserted here — never fed into arbitration, salience, selection,
//! initiative, or context injection.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::observability::event_log::EventType;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    ActivationKeyword, AllowedEffect, BOREDOM_MIN_ELAPSED_TICKS, BOREDOM_SALIENCE_THRESHOLD,
    CoherenceDecline, DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD, DeclineReason,
    DeclinedCandidate, EvidenceRef, FRUSTRATION_BLOCKED_COUNT_THRESHOLD, FunctionalSignal, Goal,
    GoalDynamicState, GoalScope, GoalStatus, Mode, ProposedGoalCandidate,
    SATISFACTION_RECENCY_WINDOW_TICKS, SignalKind, Tension, TensionPriority, VolitionEvent,
    VolitionFixture, VolitionState, apply, derive_signals,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::volition_trace_support::write_volition_trace;

const GOAL_ALPHA: &str = "texture-alpha";
const GOAL_BETA: &str = "texture-beta";
const TENSION_ID: &str = "motivational-texture";

const DERIVATION_OP: &str = "emotion-signal-derivation";
const ABSENCE_OP: &str = "emotion-signal-absence-check";
const REPORT_ARTIFACT: &str = "emotion-signal-report.md";

/// The eight fields every `emotion-signal-derivation` trace record must carry (the binding trace
/// completeness contract from `Experiment.VolitionEmotionLikeSignals.md`).
const REQUIRED_DERIVATION_FIELDS: [&str; 8] = [
    "tick",
    "signal_kind",
    "intensity",
    "thresholds_used",
    "evidence",
    "events_applied",
    "dynamic_state_snapshot",
    "artifact_or_report_reference",
];

const ALL_SIGNAL_KINDS: [SignalKind; 4] = [
    SignalKind::CoherenceDecline,
    SignalKind::Frustration,
    SignalKind::Satisfaction,
    SignalKind::Boredom,
];

pub struct VolitionEmotionSignalsExperiment;

impl Experiment for VolitionEmotionSignalsExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionEmotionSignals
    }

    fn description(&self) -> &'static str {
        "Derive the four functional signals (coherence_decline, frustration, satisfaction, \
         boredom) from recorded volition state - each on and off - and re-derive every traced \
         signal from its recorded state snapshot to prove signals depend only on recorded state; \
         offline, no arbitration/initiative wiring"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = emotion_signals_fixture();

        for scenario in scenarios() {
            run_scenario(context, &fixture, &scenario)?;
        }

        // Verify the harness against its own artifacts, in-process, so a broken derivation or a
        // signal that leaked information not present in recorded state fails the run.
        let traces = read_jsonl(&context.run_dir().join("traces.jsonl"))?;
        let events = read_jsonl(&context.run_dir().join("events.jsonl"))?;
        verify_signal_artifacts(&traces, &events, &fixture)
            .context("emotion-signal artifact verification failed")?;

        write_report(context, &traces)?;

        let derivation_count = traces
            .iter()
            .filter(|trace| trace["operation"] == DERIVATION_OP)
            .count();
        let absence_count = traces
            .iter()
            .filter(|trace| trace["operation"] == ABSENCE_OP)
            .count();

        Ok(ExperimentOutcome {
            summary: format!(
                "Drove all four functional signals on and off across {} scripted scenarios: \
                 {derivation_count} emotion-signal-derivation records (one per emitted signal) and \
                 {absence_count} emotion-signal-absence-check records. Every traced signal was \
                 re-derived from its recorded dynamic_state_snapshot and matched on kind, \
                 intensity, thresholds_used, and evidence, proving signals derive only from \
                 recorded state.",
                scenarios().len()
            ),
            observations: vec![
                "Each signal is derived by the pure derive_signals over a state rebuilt purely by replaying recorded VolitionEvents; the harness records, re-parses, and re-derives without any live model call.".to_string(),
                "GoalSatisfied both raises satisfaction and clears the goal's frustration bookkeeping (blocked_count reset), so the same goal that read as frustration reads as satisfaction with no residual frustration.".to_string(),
                "Absence is evidence-backed: every off-case snapshot re-derives to no signal of the asserted kind, so cold-start boredom and progress-only satisfaction are provably absent from the artifacts.".to_string(),
                "Signals are consumed nowhere but this harness and its trace records; no arbitration, salience, selection, initiative, or context-injection path reads them.".to_string(),
            ],
            failure_modes: vec![
                "Thresholds are chosen for fixture coverage (block twice = frustration, tick 3 = elapsed boredom); they are not validated as live defaults.".to_string(),
                "The scenarios are deterministic scripts; a live lifecycle stream could interleave signals this fixture keeps separate.".to_string(),
            ],
            follow_up_questions: vec![
                "Does live operator review prefer the label `boredom` or a less anthropomorphic display label?".to_string(),
            ],
            decision_candidates: vec![
                "Confirm the emotion-signal-derivation trace shape (this experiment's contract) as the durable functional-signal trace format.".to_string(),
            ],
            extra_artifacts: vec![REPORT_ARTIFACT.to_string()],
        })
    }
}

/// A two-goal, single-tension fixture. Purpose-built so one goal (`texture-alpha`) can be driven
/// through block/satisfy/decline while the other (`texture-beta`) serves as a boredom suppressor
/// and as the conflict target a coherence decline names.
fn emotion_signals_fixture() -> VolitionFixture {
    let tensions = vec![Tension {
        id: TENSION_ID.to_string(),
        title: "Motivational texture".to_string(),
        summary: "Surfaces motivational texture as evidence-backed functional signals.".to_string(),
        priority_bias: TensionPriority::Medium,
        arbitration_tier: 6,
        focused_bias: 0,
        exploratory_bias: 0,
    }];
    let goals = vec![goal(GOAL_ALPHA, 60), goal(GOAL_BETA, 60)];
    VolitionFixture {
        tensions,
        goals,
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    }
}

fn goal(id: &str, base_priority: u8) -> Goal {
    Goal {
        id: id.to_string(),
        title: id.to_string(),
        summary: id.to_string(),
        tension_ids: vec![TENSION_ID.to_string()],
        status: GoalStatus::Accepted,
        scope: GoalScope::Session,
        base_priority,
        activation_keywords: vec![ActivationKeyword::normal("texture")],
        allowed_effects: vec![AllowedEffect::Reflect],
        satisfaction_condition_summary: id.to_string(),
        evidence_refs: vec![
            "docs/Experiments/Experiment.VolitionEmotionLikeSignals.md".to_string(),
        ],
        estimated_tokens: 10,
        source_reference: "docs/Experiments/Experiment.VolitionEmotionLikeSignals.md".to_string(),
        visibility: qsf_volition::GoalVisibility::Conscious,
    }
}

/// One scripted scenario: a deterministic event list plus the exact set of signal kinds expected
/// to be present when `derive_signals` is run over the resulting state. Everything else is an
/// asserted absence.
struct Scenario {
    label: &'static str,
    intent: &'static str,
    events: Vec<VolitionEvent>,
    expected_present: &'static [SignalKind],
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // frustration ON: activated then blocked past the threshold.
        Scenario {
            label: "frustration-onset",
            intent: "An activated goal blocked twice reaches the frustration blocked-count threshold.",
            events: vec![
                activated(GOAL_ALPHA, 1),
                blocked(GOAL_ALPHA, 2),
                blocked(GOAL_ALPHA, 3),
            ],
            expected_present: &[SignalKind::Frustration],
        },
        // frustration OFF: a single block stays below the threshold.
        Scenario {
            label: "frustration-below-threshold",
            intent: "One block is ordinary friction, below the frustration threshold.",
            events: vec![activated(GOAL_ALPHA, 1), blocked(GOAL_ALPHA, 2)],
            expected_present: &[],
        },
        // satisfaction ON, and the same goal's frustration is cleared by GoalSatisfied.
        Scenario {
            label: "satisfaction-clears-frustration",
            intent: "A goal blocked past threshold then satisfied reads as satisfaction with its \
                     frustration bookkeeping reset; beta stays engaged to isolate boredom.",
            events: vec![
                activated(GOAL_BETA, 1),
                activated(GOAL_ALPHA, 1),
                blocked(GOAL_ALPHA, 2),
                blocked(GOAL_ALPHA, 3),
                satisfied(GOAL_ALPHA, 4, "trace: alpha satisfaction closed"),
            ],
            expected_present: &[SignalKind::Satisfaction],
        },
        // satisfaction OFF: progress-only evidence must not read as satisfaction.
        Scenario {
            label: "satisfaction-progress-only",
            intent: "Progress evidence does not populate satisfaction-only state, so no \
                     satisfaction is derived.",
            events: vec![
                activated(GOAL_ALPHA, 1),
                progress(GOAL_ALPHA, 2, "trace: alpha progress noted"),
            ],
            expected_present: &[],
        },
        // coherence_decline ON: a coherence-engine rejection carrying a CoherenceDecline.
        Scenario {
            label: "coherence-decline",
            intent: "A GoalCandidateRejected carrying a CoherenceDecline records a declined \
                     candidate the signal names.",
            events: vec![
                candidate_added("texture-detour", "Chase an off-continuity detour", 1),
                candidate_rejected_coherence(
                    "texture-detour",
                    DeclineReason::ConflictingGoal {
                        goal_id: GOAL_BETA.to_string(),
                    },
                    "would relitigate ground texture-beta already governs",
                    2,
                ),
            ],
            expected_present: &[SignalKind::CoherenceDecline],
        },
        // coherence_decline OFF: a non-coherence rejection keeps no declined-candidate state.
        Scenario {
            label: "coherence-no-decline",
            intent: "A rejection without a CoherenceDecline (e.g. reflection review) records no \
                     declined candidate, so no coherence_decline is derived.",
            events: vec![
                candidate_added("texture-review-reject", "A candidate declined on review", 1),
                candidate_rejected_plain("texture-review-reject", 2),
            ],
            expected_present: &[],
        },
        // boredom OFF: cold start (no activity, low tick) is not boredom.
        Scenario {
            label: "boredom-cold-start",
            intent: "A fresh session with no activation and a low tick is a cold start, not \
                     boredom.",
            events: vec![],
            expected_present: &[],
        },
        // boredom ON via the elapsed-ticks guard: idle goals past the min elapsed tick.
        Scenario {
            label: "boredom-elapsed",
            intent: "With no activation but the clock past the min elapsed tick, the whole idle \
                     goal set reads as boredom.",
            events: vec![tick_advanced(BOREDOM_MIN_ELAPSED_TICKS)],
            expected_present: &[SignalKind::Boredom],
        },
        // boredom ON via the prior-activation guard (co-occurring with satisfaction), at a low tick.
        Scenario {
            label: "boredom-prior-activation",
            intent: "A goal activated then satisfied returns to zero salience but keeps its prior \
                     activation, so the idle set reads as boredom even below the elapsed guard; \
                     satisfaction co-occurs.",
            events: vec![
                activated(GOAL_ALPHA, 1),
                satisfied(GOAL_ALPHA, 2, "trace: alpha satisfied before idling"),
            ],
            expected_present: &[SignalKind::Satisfaction, SignalKind::Boredom],
        },
    ]
}

fn activated(goal_id: &str, tick: u64) -> VolitionEvent {
    VolitionEvent::GoalActivated {
        goal_id: goal_id.to_string(),
        tick,
    }
}

fn blocked(goal_id: &str, tick: u64) -> VolitionEvent {
    VolitionEvent::GoalBlocked {
        goal_id: goal_id.to_string(),
        tick,
    }
}

fn satisfied(goal_id: &str, tick: u64, evidence: &str) -> VolitionEvent {
    VolitionEvent::GoalSatisfied {
        goal_id: goal_id.to_string(),
        evidence: EvidenceRef::try_new(evidence).unwrap(),
        tick,
    }
}

fn progress(goal_id: &str, tick: u64, evidence: &str) -> VolitionEvent {
    VolitionEvent::GoalProgressObserved {
        goal_id: goal_id.to_string(),
        evidence: EvidenceRef::try_new(evidence).unwrap(),
        tick,
    }
}

fn tick_advanced(tick: u64) -> VolitionEvent {
    VolitionEvent::TickAdvanced { tick }
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

fn candidate_rejected_plain(id: &str, tick: u64) -> VolitionEvent {
    VolitionEvent::GoalCandidateRejected {
        goal_id: id.to_string(),
        reason: "declined on reflection review".to_string(),
        coherence_decline: None,
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
        vec![EvidenceRef::try_new("scripted texture candidate").unwrap()],
        "formed from scripted texture scenario".to_string(),
        vec!["texture".to_string()],
    )
    .unwrap()
}

/// The state slice `derive_signals` reads. Serialized as `dynamic_state_snapshot` and
/// deserialized to reconstruct a `VolitionState` for re-derivation. It carries the full
/// per-goal [`GoalDynamicState`] (so every field the signals read — `blocked_count`,
/// `last_blocked_tick`, `last_satisfied_tick`, `last_satisfied_evidence_ref`, `salience`,
/// `status`, `last_activated_tick`) plus the state `tick` and `declined_candidates`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct SignalStateSnapshot {
    tick: u64,
    goals: BTreeMap<String, GoalDynamicState>,
    declined_candidates: Vec<DeclinedCandidate>,
}

fn snapshot_of(state: &VolitionState) -> SignalStateSnapshot {
    SignalStateSnapshot {
        tick: state.tick,
        goals: state.goals.clone(),
        declined_candidates: state.declined_candidates.clone(),
    }
}

/// Rebuilds a `VolitionState` from a recorded snapshot. Only the fields `derive_signals` reads
/// are restored; the rest take empty/default values, which the signal derivation ignores.
fn state_from_snapshot(snapshot: SignalStateSnapshot) -> VolitionState {
    VolitionState {
        tick: snapshot.tick,
        goals: snapshot.goals,
        pending_candidates: Vec::new(),
        accepted_candidates: BTreeMap::new(),
        mode: Mode::default(),
        declined_candidates: snapshot.declined_candidates,
    }
}

/// Builds `VolitionState` by replaying `events` over a fresh fixture-seeded state.
fn replay(events: &[VolitionEvent], fixture: &VolitionFixture) -> VolitionState {
    let mut state = VolitionState::from_fixture(fixture);
    for event in events {
        state = apply(state, event.clone());
    }
    state
}

/// The threshold constants each signal kind uses in its derivation. Written into every trace and
/// recomputed during verification, so a mismatch surfaces if the trace and the derivation drift.
fn thresholds_used_for(kind: SignalKind) -> Value {
    match kind {
        // Recency-based; no gating threshold constant.
        SignalKind::CoherenceDecline => json!({}),
        SignalKind::Frustration => json!({
            "frustration_blocked_count_threshold": FRUSTRATION_BLOCKED_COUNT_THRESHOLD,
        }),
        SignalKind::Satisfaction => json!({
            "satisfaction_recency_window_ticks": SATISFACTION_RECENCY_WINDOW_TICKS,
        }),
        SignalKind::Boredom => json!({
            "boredom_salience_threshold": BOREDOM_SALIENCE_THRESHOLD,
            "boredom_min_elapsed_ticks": BOREDOM_MIN_ELAPSED_TICKS,
        }),
    }
}

fn run_scenario(
    context: &mut RunContext,
    fixture: &VolitionFixture,
    scenario: &Scenario,
) -> anyhow::Result<()> {
    let state = replay(&scenario.events, fixture);
    let signals = derive_signals(&state, fixture);

    let present_kinds: Vec<SignalKind> = signals.iter().map(|signal| signal.kind).collect();
    ensure!(
        present_kinds == scenario.expected_present,
        "scenario `{}` derived {present_kinds:?}, expected {:?}",
        scenario.label,
        scenario.expected_present
    );

    let snapshot = json!(snapshot_of(&state));
    let events_json = json!(scenario.events);

    for signal in &signals {
        write_derivation_record(context, scenario, &state, signal, &snapshot, &events_json)?;
    }

    let expected_absent: Vec<SignalKind> = ALL_SIGNAL_KINDS
        .iter()
        .copied()
        .filter(|kind| !present_kinds.contains(kind))
        .collect();
    write_absence_record(
        context,
        scenario,
        &state,
        &present_kinds,
        &expected_absent,
        &snapshot,
        &events_json,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_derivation_record(
    context: &mut RunContext,
    scenario: &Scenario,
    state: &VolitionState,
    signal: &FunctionalSignal,
    snapshot: &Value,
    events_json: &Value,
) -> anyhow::Result<()> {
    let kind_json = json!(signal.kind);
    let kind_str = kind_json.as_str().unwrap_or_default().to_string();
    let details = json!({
        "scenario": scenario.label,
        "intent": scenario.intent,
        "tick": state.tick,
        "signal_kind": kind_json,
        "intensity": signal.intensity,
        "thresholds_used": thresholds_used_for(signal.kind),
        "evidence": json!(signal.evidence),
        "events_applied": events_json,
        "dynamic_state_snapshot": snapshot,
        "artifact_or_report_reference":
            format!("{DERIVATION_OP}#scenario={}&kind={kind_str}", scenario.label),
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
            "signal_kind": kind_str,
            "tick": state.tick,
        }),
        Some(trace_id),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_absence_record(
    context: &mut RunContext,
    scenario: &Scenario,
    state: &VolitionState,
    present_kinds: &[SignalKind],
    expected_absent: &[SignalKind],
    snapshot: &Value,
    events_json: &Value,
) -> anyhow::Result<()> {
    let details = json!({
        "scenario": scenario.label,
        "intent": scenario.intent,
        "tick": state.tick,
        "signals_present": present_kinds.iter().map(|k| json!(k)).collect::<Vec<_>>(),
        "expected_absent_kinds": expected_absent.iter().map(|k| json!(k)).collect::<Vec<_>>(),
        "events_applied": events_json,
        "dynamic_state_snapshot": snapshot,
        "artifact_or_report_reference":
            format!("{ABSENCE_OP}#scenario={}", scenario.label),
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
            "expected_absent_kinds": expected_absent.iter().map(|k| json!(k)).collect::<Vec<_>>(),
            "tick": state.tick,
        }),
        Some(trace_id),
    )?;
    Ok(())
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

/// Parses the run's `traces.jsonl` / `events.jsonl` and enforces the trace-completeness contract:
/// required fields on every derivation record, a genuine re-derivation of each traced signal from
/// its recorded snapshot, absence re-derivation for every off-case, `TraceRecorded` linkage, and
/// on-and-off coverage of all four signal kinds. Pure over its inputs so it is unit-testable.
fn verify_signal_artifacts(
    traces: &[Value],
    events: &[Value],
    fixture: &VolitionFixture,
) -> anyhow::Result<()> {
    let derivations: Vec<&Value> = traces
        .iter()
        .filter(|trace| trace["operation"] == DERIVATION_OP)
        .collect();
    let absences: Vec<&Value> = traces
        .iter()
        .filter(|trace| trace["operation"] == ABSENCE_OP)
        .collect();

    ensure!(
        !derivations.is_empty(),
        "no emotion-signal-derivation trace records were written"
    );

    for record in &derivations {
        verify_derivation_record(record, fixture)?;
    }
    for record in &absences {
        verify_absence_record(record, fixture)?;
    }

    verify_trace_recorded_linkage(traces, events)?;
    verify_on_and_off_coverage(&derivations, &absences)?;
    Ok(())
}

fn verify_derivation_record(record: &Value, fixture: &VolitionFixture) -> anyhow::Result<()> {
    let details = &record["details"];
    let scenario = details["scenario"].as_str().unwrap_or("<unknown>");
    for field in REQUIRED_DERIVATION_FIELDS {
        let value = details.get(field);
        ensure!(
            value.is_some() && !value.unwrap().is_null(),
            "derivation record for scenario `{scenario}` is missing required field `{field}`"
        );
    }

    let state = reconstruct_and_cross_check(details, fixture, scenario)?;
    let derived = derive_signals(&state, fixture);

    let traced_kind = &details["signal_kind"];
    let traced_evidence = &details["evidence"];
    let matched = derived
        .iter()
        .find(|signal| {
            &json!(signal.kind) == traced_kind
                && &serde_json::to_value(&signal.evidence).unwrap() == traced_evidence
        })
        .with_context(|| {
            format!(
                "re-derivation from the recorded snapshot produced no signal matching the traced \
                 kind/evidence for scenario `{scenario}`"
            )
        })?;

    ensure!(
        json!(matched.intensity) == details["intensity"],
        "intensity mismatch on re-derivation for scenario `{scenario}`: traced {}, re-derived {}",
        details["intensity"],
        json!(matched.intensity)
    );
    ensure!(
        thresholds_used_for(matched.kind) == details["thresholds_used"],
        "thresholds_used mismatch on re-derivation for scenario `{scenario}`"
    );
    Ok(())
}

fn verify_absence_record(record: &Value, fixture: &VolitionFixture) -> anyhow::Result<()> {
    let details = &record["details"];
    let scenario = details["scenario"].as_str().unwrap_or("<unknown>");
    for field in [
        "tick",
        "expected_absent_kinds",
        "signals_present",
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
    let derived = derive_signals(&state, fixture);
    let derived_kinds: HashSet<Value> = derived.iter().map(|signal| json!(signal.kind)).collect();

    let expected_absent = details["expected_absent_kinds"]
        .as_array()
        .with_context(|| {
            format!("absence record for scenario `{scenario}` has non-array expected_absent_kinds")
        })?;
    for kind in expected_absent {
        ensure!(
            !derived_kinds.contains(kind),
            "scenario `{scenario}` asserted {kind} absent, but re-derivation from its snapshot \
             produced it"
        );
    }

    let present: HashSet<Value> = details["signals_present"]
        .as_array()
        .map(|values| values.iter().cloned().collect())
        .unwrap_or_default();
    ensure!(
        present == derived_kinds,
        "signals_present {:?} does not match re-derived kinds {:?} for scenario `{scenario}`",
        present,
        derived_kinds
    );
    Ok(())
}

/// Reconstructs the state slice two ways — deserializing the recorded `dynamic_state_snapshot`,
/// and independently replaying the recorded `events_applied` — and asserts they agree, so the
/// snapshot the re-derivation trusts is provably the state those events produce.
fn reconstruct_and_cross_check(
    details: &Value,
    fixture: &VolitionFixture,
    scenario: &str,
) -> anyhow::Result<VolitionState> {
    let snapshot: SignalStateSnapshot =
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
        "expected one TraceRecorded event per emotion-signal-derivation trace: {} events vs {} traces",
        linked.len(),
        derivation_trace_ids.len()
    );
    for event in linked {
        let trace_id = event["trace_id"]
            .as_str()
            .context("TraceRecorded event has no linked trace_id")?;
        ensure!(
            derivation_trace_ids.contains(trace_id),
            "TraceRecorded event links trace_id {trace_id}, which is not an \
             emotion-signal-derivation trace"
        );
    }
    Ok(())
}

fn verify_on_and_off_coverage(derivations: &[&Value], absences: &[&Value]) -> anyhow::Result<()> {
    for kind in ALL_SIGNAL_KINDS {
        let kind_json = json!(kind);
        ensure!(
            derivations
                .iter()
                .any(|record| record["details"]["signal_kind"] == kind_json),
            "no on-case: signal {kind_json} was never derived in any scenario"
        );
        ensure!(
            absences.iter().any(|record| {
                record["details"]["expected_absent_kinds"]
                    .as_array()
                    .is_some_and(|kinds| kinds.contains(&kind_json))
            }),
            "no off-case: signal {kind_json} was never asserted absent in any scenario"
        );
    }
    Ok(())
}

fn write_report(context: &RunContext, traces: &[Value]) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Volition Emotion-Like Signals\n\n");
    md.push_str(
        "Offline harness: scripted lifecycle-event sequences drive each functional signal on and \
         off; the pure `derive_signals` reads only recorded state, and every traced signal is \
         re-derived from its recorded `dynamic_state_snapshot`. See \
         `docs/Experiments/Experiment.VolitionEmotionLikeSignals.md`.\n\n",
    );

    for kind in ALL_SIGNAL_KINDS {
        let kind_json = json!(kind);
        let kind_label = kind_json.as_str().unwrap_or_default();
        md.push_str(&format!("## `{kind_label}`\n\n"));

        md.push_str("Present in:\n\n");
        let mut any_present = false;
        for record in traces
            .iter()
            .filter(|trace| trace["operation"] == DERIVATION_OP)
            .filter(|trace| trace["details"]["signal_kind"] == kind_json)
        {
            let details = &record["details"];
            any_present = true;
            md.push_str(&format!(
                "- `{}` (tick {}, intensity {}) — {}\n",
                details["scenario"].as_str().unwrap_or_default(),
                details["tick"],
                details["intensity"],
                details["intent"].as_str().unwrap_or_default(),
            ));
        }
        if !any_present {
            md.push_str("- (none)\n");
        }

        md.push_str("\nAsserted absent in:\n\n");
        let mut any_absent = false;
        for record in traces
            .iter()
            .filter(|trace| trace["operation"] == ABSENCE_OP)
            .filter(|trace| {
                trace["details"]["expected_absent_kinds"]
                    .as_array()
                    .is_some_and(|kinds| kinds.contains(&kind_json))
            })
        {
            let details = &record["details"];
            any_absent = true;
            md.push_str(&format!(
                "- `{}` (tick {}) — {}\n",
                details["scenario"].as_str().unwrap_or_default(),
                details["tick"],
                details["intent"].as_str().unwrap_or_default(),
            ));
        }
        if !any_absent {
            md.push_str("- (none)\n");
        }
        md.push('\n');
    }

    md.push_str("## Human Verification Checklist\n\n");
    md.push_str(
        "- [ ] Every signal row is explained by its `emotion-signal-derivation` trace record's \
         evidence alone.\n",
    );
    md.push_str(
        "- [ ] Each signal appears on and off, and each off-case snapshot re-derives to no signal \
         of that kind.\n",
    );
    md.push_str(
        "- [ ] `satisfaction-clears-frustration` shows the same goal reading as satisfaction with \
         frustration cleared.\n",
    );
    md.push_str("- [ ] No path outside this harness consumes the derived signals.\n");

    fs::write(context.run_dir().join(REPORT_ARTIFACT), md).with_context(|| {
        format!(
            "failed to write emotion-signal report for run {}",
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
        ABSENCE_OP, BOREDOM_SALIENCE_THRESHOLD, DERIVATION_OP, FRUSTRATION_BLOCKED_COUNT_THRESHOLD,
        SATISFACTION_RECENCY_WINDOW_TICKS, SignalKind, emotion_signals_fixture,
        thresholds_used_for, verify_signal_artifacts,
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
            std::env::temp_dir().join(format!("qsf-emotion-signals-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionEmotionSignals).unwrap();
        let traces = parse_jsonl(&summary.run_dir.join("traces.jsonl"));
        let events = parse_jsonl(&summary.run_dir.join("events.jsonl"));
        (traces, events, base_dir, summary.run_dir)
    }

    #[test]
    fn experiment_is_registered_and_dispatches() {
        let experiments = available_experiments();
        let entry = experiments
            .iter()
            .find(|experiment| experiment.id == ExperimentName::VolitionEmotionSignals.id())
            .expect("volition-emotion-signals must be registered");
        assert_eq!(
            ExperimentName::VolitionEmotionSignals.to_string(),
            "volition-emotion-signals"
        );
        assert!(entry.description.contains("functional signals"));

        let parsed: ExperimentName = "volition-emotion-signals".parse().unwrap();
        assert_eq!(parsed, ExperimentName::VolitionEmotionSignals);
    }

    #[test]
    fn experiment_runs_and_produces_artifacts() {
        let (traces, _events, base_dir, run_dir) = run();
        assert!(traces.iter().any(|t| t["operation"] == DERIVATION_OP));
        assert!(traces.iter().any(|t| t["operation"] == ABSENCE_OP));

        let report = fs::read_to_string(run_dir.join("emotion-signal-report.md")).unwrap();
        assert!(report.contains("Human Verification Checklist"));
        assert!(report.contains("`frustration`"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn thresholds_used_are_per_kind() {
        assert_eq!(
            thresholds_used_for(SignalKind::Frustration)["frustration_blocked_count_threshold"],
            FRUSTRATION_BLOCKED_COUNT_THRESHOLD
        );
        assert_eq!(
            thresholds_used_for(SignalKind::Satisfaction)["satisfaction_recency_window_ticks"],
            SATISFACTION_RECENCY_WINDOW_TICKS
        );
        assert_eq!(
            thresholds_used_for(SignalKind::Boredom)["boredom_salience_threshold"],
            BOREDOM_SALIENCE_THRESHOLD
        );
        // coherence_decline is recency-based; no gating threshold constant.
        assert_eq!(
            thresholds_used_for(SignalKind::CoherenceDecline),
            serde_json::json!({})
        );
    }

    #[test]
    fn verification_passes_on_the_real_run_artifacts() {
        let (traces, events, base_dir, _run_dir) = run();
        verify_signal_artifacts(&traces, &events, &emotion_signals_fixture()).unwrap();
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn verification_rejects_a_tampered_intensity() {
        let (mut traces, events, base_dir, _run_dir) = run();
        // Corrupt one derivation record's intensity: re-derivation from the untouched snapshot
        // must no longer agree, proving the check is a real recomputation, not self-comparison.
        let record = traces
            .iter_mut()
            .find(|trace| trace["operation"] == DERIVATION_OP)
            .unwrap();
        record["details"]["intensity"] = serde_json::json!(0.123_456);

        let error = verify_signal_artifacts(&traces, &events, &emotion_signals_fixture())
            .expect_err("tampered intensity must fail verification");
        assert!(error.to_string().contains("intensity mismatch"));
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn verification_rejects_a_missing_required_field() {
        let (mut traces, events, base_dir, _run_dir) = run();
        let record = traces
            .iter_mut()
            .find(|trace| trace["operation"] == DERIVATION_OP)
            .unwrap();
        record["details"]
            .as_object_mut()
            .unwrap()
            .remove("thresholds_used");

        let error = verify_signal_artifacts(&traces, &events, &emotion_signals_fixture())
            .expect_err("missing required field must fail verification");
        assert!(error.to_string().contains("missing required field"));
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn verification_rejects_a_leaked_absence() {
        let (traces, events, base_dir, _run_dir) = run();
        // Tamper an absence record so it claims a kind is absent that its snapshot actually
        // derives, proving the off-case check re-derives rather than trusting the label.
        let mut traces = traces;
        let record = traces
            .iter_mut()
            .find(|trace| {
                trace["operation"] == ABSENCE_OP
                    && trace["details"]["scenario"] == "boredom-elapsed"
            })
            .unwrap();
        record["details"]["expected_absent_kinds"] = serde_json::json!(["boredom"]);

        let error = verify_signal_artifacts(&traces, &events, &emotion_signals_fixture())
            .expect_err("a falsely-claimed absence must fail verification");
        assert!(error.to_string().contains("asserted"));
        fs::remove_dir_all(base_dir).unwrap();
    }
}
