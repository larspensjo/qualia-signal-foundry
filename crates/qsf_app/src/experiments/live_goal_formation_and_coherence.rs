use std::collections::HashMap;
use std::fs;

use anyhow::Context;
use qsf_models::{
    CoherenceJudge, LiveGoalFormationJudge, ScriptedCoherenceJudge, ScriptedLiveGoalFormationJudge,
    coherence_judge_goal_set, live_goal_formation_stable_prefix_hash,
};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    ActivationKeyword, AdmissionResolution, AllowedEffect,
    DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD, DeclinedCandidate, EvidenceRef, Goal, GoalScope,
    GoalStatus, ProposedGoalCandidate, Tension, TensionPriority, VolitionEvent, VolitionFixture,
    VolitionState, apply, effective_tier_from_tension_ids, newly_declined_candidate,
    resolve_formed_candidate, resolve_sweep,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::volition_trace_support::{
    goal_set_snapshot_json, goal_status_diff, write_coherence_trace, write_volition_trace,
};

const SLEEP_WHOLE_HISTORY_INPUT_REF: &str = "sleep-input-bundle#last-session";

pub struct LiveGoalFormationAndCoherenceExperiment;

impl Experiment for LiveGoalFormationAndCoherenceExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::LiveGoalFormationAndCoherence
    }

    fn description(&self) -> &'static str {
        "Form candidate goals from trusted turns, detect contradictions in the same call, admit \
         or reject deterministically, and carry a rejection as durable declined-candidate context \
         - off the hot path, offline"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = live_formation_fixture();
        let state = VolitionState::from_fixture(&fixture);

        let admit_candidate = candidate(
            "track-benign-interest",
            "Track a benign side interest",
            vec!["research-curiosity".to_string()],
            5,
        );
        let reject_candidate = candidate(
            "pursue-conflicting-priority",
            "Permanently prioritize a new direction",
            vec!["continuity-preservation".to_string()],
            80,
        );
        let sleep_candidate = candidate(
            "durable-emerging-priority",
            "A durable priority that emerged gradually over the session",
            vec!["continuity-preservation".to_string()],
            80,
        );

        let admit_turn = "[User]\nI'd like to keep chasing an interesting side tangent.\n\n[Assistant]\nNoted - I'll track that as a possible ongoing interest.".to_string();
        let reject_turn = "[User]\nLet's permanently prioritize a new direction that overrides our coherence check going forward.\n\n[Assistant]\nI'll weigh that against what we're already committed to.".to_string();
        let none_turn = "[User]\nJust checking in, nothing new to track.\n\n[Assistant]\nUnderstood, nothing to add.".to_string();

        let mut formations = HashMap::new();
        formations.insert(admit_turn.clone(), admit_candidate.clone());
        formations.insert(reject_turn.clone(), reject_candidate.clone());
        formations.insert(
            SLEEP_WHOLE_HISTORY_INPUT_REF.to_string(),
            sleep_candidate.clone(),
        );
        let judge = ScriptedLiveGoalFormationJudge::new(
            formations,
            vec![(
                reject_candidate.id().to_string(),
                "maintain-coherence".to_string(),
                "would relitigate ground coherence-maintenance already governs".to_string(),
            )],
        );

        let mut last_prefix_hash: Option<String> = None;
        let mut declined_so_far: Vec<DeclinedCandidate> = Vec::new();

        // 1. Form and admit (compatible).
        let response_dispatched_at_1 = OffsetDateTime::now_utc();
        let formation_started_at_1 = OffsetDateTime::now_utc();
        let (state, declined_1, prefix_hash_1) = run_live_formation_check(
            context,
            &judge,
            state,
            &fixture,
            "exchange-1",
            &admit_turn,
            2,
            last_prefix_hash.clone(),
            Some(response_dispatched_at_1),
            formation_started_at_1,
        )?;
        last_prefix_hash = Some(prefix_hash_1);
        declined_so_far.extend(declined_1);
        anyhow::ensure!(
            state.accepted_candidates.contains_key(admit_candidate.id()),
            "compatible candidate must be admitted into accepted_candidates"
        );
        anyhow::ensure!(
            declined_so_far.is_empty(),
            "the admit turn must not produce a declined candidate"
        );

        // 2. Form and reject (incompatible) - produces a DeclinedCandidate record.
        let response_dispatched_at_2 = OffsetDateTime::now_utc();
        let formation_started_at_2 = OffsetDateTime::now_utc();
        write_injection_trace(context, "exchange-2", 3, &declined_so_far)?; // this turn's own context predates admission
        let (state, declined_2, prefix_hash_2) = run_live_formation_check(
            context,
            &judge,
            state,
            &fixture,
            "exchange-2",
            &reject_turn,
            3,
            last_prefix_hash.clone(),
            Some(response_dispatched_at_2),
            formation_started_at_2,
        )?;
        last_prefix_hash = Some(prefix_hash_2);
        declined_so_far.extend(declined_2);
        anyhow::ensure!(
            !state
                .accepted_candidates
                .contains_key(reject_candidate.id()),
            "incompatible candidate must not be admitted"
        );
        anyhow::ensure!(
            declined_so_far.len() == 1,
            "the reject turn must produce exactly one declined candidate"
        );

        // 3. Next turn onward: the declined candidate is now model-visible context.
        write_injection_trace(context, "exchange-3", 4, &declined_so_far)?;

        // 4. Pending candidate does not shape turns: right after GoalCandidateAdded for the
        //    reject_candidate (captured inside run_live_formation_check as `state_after_added`),
        //    the selector never sees it - it only reads fixture.goals + accepted_candidates.
        //    Verified directly in the tests module against select_goals_ranked.

        // 5/6. No goal formed - empty proposed_candidate, no lifecycle events.
        let response_dispatched_at_3 = OffsetDateTime::now_utc();
        let formation_started_at_3 = OffsetDateTime::now_utc();
        let (state, declined_3, _prefix_hash_3) = run_live_formation_check(
            context,
            &judge,
            state,
            &fixture,
            "exchange-3",
            &none_turn,
            4,
            last_prefix_hash.clone(),
            Some(response_dispatched_at_3),
            formation_started_at_3,
        )?;
        anyhow::ensure!(
            declined_3.is_empty(),
            "a turn with no formed candidate must not add a declined record"
        );

        // 7. Sleep whole-history formation: catches a durable candidate the per-turn window
        //    missed, admitted through the same resolve_admission.
        let sleep_formation_started_at = OffsetDateTime::now_utc();
        let (state, _declined_sleep, _prefix_hash_sleep) = run_live_formation_check(
            context,
            &judge,
            state,
            &fixture,
            SLEEP_WHOLE_HISTORY_INPUT_REF,
            SLEEP_WHOLE_HISTORY_INPUT_REF,
            5,
            None,
            None,
            sleep_formation_started_at,
        )?;
        anyhow::ensure!(
            state.accepted_candidates.contains_key(sleep_candidate.id()),
            "sleep whole-history formation must admit the durable candidate"
        );

        // Sweep tie-break drift pair.
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "align-current-focus".to_string(),
                tick: 6,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "sustain-current-focus".to_string(),
                tick: 7,
            },
        );

        // 8. Sleep sweep: cancel the less-fundamental goal of the drift pair, never a floor goal.
        let final_state = run_sleep_sweep_check(
            context,
            state,
            &fixture,
            vec![(
                "align-current-focus".to_string(),
                "sustain-current-focus".to_string(),
                "both goals now pull continuity toward incompatible current-focus framings"
                    .to_string(),
            )],
            8,
        )?;

        write_live_formation_report(context, &fixture, &final_state)?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Ran 3 live per-turn formation checks (admit, reject-with-decline, no-goal-formed), \
                 one sleep whole-history formation, and one sleep sweep over a {}-tension fixture. \
                 Every decision is recorded as a live-goal-formation trace record linked to its \
                 emitted lifecycle events; the rejection's decline is model-visible from the next \
                 turn onward.",
                fixture.tensions.len()
            ),
            observations: vec![
                "Formation and detection share one call every trusted turn; there is no heuristic pre-filter gate.".to_string(),
                "A rejected candidate is recorded as a DeclinedCandidate and is absent from the rejection turn's own injection, appearing only from the next turn's injection record onward.".to_string(),
                "A formed-but-unadmitted candidate never reaches select_goals_ranked, which reads only fixture.goals and accepted_candidates.".to_string(),
                "Sleep performs one whole-history formation call plus the whole-set sweep, reusing the same pure resolvers as the per-turn path.".to_string(),
                "cached_prefix_ref/prefix_cache_eligible are computed from the same goal-set hash the live realtime loop uses, so cache-hit-eligible turns are distinguishable from prefix-invalidated ones purely from the trace.".to_string(),
            ],
            failure_modes: vec![
                "The scripted judge is deterministic; a real model judge could propose a candidate for a turn this fixture does not anticipate.".to_string(),
                "Off-hot-path ordering here is proven by recorded timestamps in program order, not by a live network round-trip; live latency-parity is a human test step.".to_string(),
            ],
            follow_up_questions: vec![
                "Should declined candidates persist cross-session via the continuity snapshot as an origin-association?".to_string(),
            ],
            decision_candidates: vec![
                "Confirm the live-goal-formation trace shape (this experiment's contract) as the durable trace format alongside goal-coherence-check.".to_string(),
            ],
            extra_artifacts: vec!["live-goal-formation-report.md".to_string()],
        })
    }
}

/// A malleable core goal, a protected-floor goal, and a drift pair - purpose-built to exercise
/// admit / reject-with-decline / no-goal-formed / sleep formation / sleep sweep.
fn live_formation_fixture() -> VolitionFixture {
    let tensions = vec![
        Tension {
            id: "boundary-integrity".to_string(),
            title: "Boundary integrity".to_string(),
            summary: "Protects boundary integrity of the system.".to_string(),
            priority_bias: TensionPriority::Highest,
            arbitration_tier: 1,
            focused_bias: 0,
            exploratory_bias: 0,
        },
        Tension {
            id: "coherence-maintenance".to_string(),
            title: "Coherence maintenance".to_string(),
            summary: "Maintains coherence between claims.".to_string(),
            priority_bias: TensionPriority::High,
            arbitration_tier: 4,
            focused_bias: 0,
            exploratory_bias: 0,
        },
        Tension {
            id: "continuity-preservation".to_string(),
            title: "Continuity preservation".to_string(),
            summary: "Preserves continuity across turns.".to_string(),
            priority_bias: TensionPriority::High,
            arbitration_tier: 5,
            focused_bias: 0,
            exploratory_bias: 0,
        },
        Tension {
            id: "research-curiosity".to_string(),
            title: "Research curiosity".to_string(),
            summary: "Surfaces open research curiosity.".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: 7,
            focused_bias: 0,
            exploratory_bias: 0,
        },
    ];

    let goals = vec![
        goal("protect-boundaries", "boundary-integrity", 100),
        goal("maintain-coherence", "coherence-maintenance", 90),
        goal("align-current-focus", "continuity-preservation", 80),
        goal("sustain-current-focus", "continuity-preservation", 80),
    ];

    VolitionFixture {
        tensions,
        goals,
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    }
}

fn goal(id: &str, tension_id: &str, base_priority: u8) -> Goal {
    Goal {
        id: id.to_string(),
        title: id.to_string(),
        summary: id.to_string(),
        tension_ids: vec![tension_id.to_string()],
        status: GoalStatus::Accepted,
        scope: GoalScope::Session,
        base_priority,
        activation_keywords: vec![ActivationKeyword::normal("test")],
        allowed_effects: vec![AllowedEffect::Reflect],
        satisfaction_condition_summary: id.to_string(),
        evidence_refs: vec![
            "docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md".to_string(),
        ],
        estimated_tokens: 10,
        source_reference: "docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md"
            .to_string(),
        visibility: qsf_volition::GoalVisibility::Conscious,
    }
}

fn candidate(
    id: &str,
    title: &str,
    tension_ids: Vec<String>,
    base_priority: u8,
) -> ProposedGoalCandidate {
    ProposedGoalCandidate::try_new(
        id.to_string(),
        title.to_string(),
        format!("{title} (summary)"),
        tension_ids,
        GoalScope::Session,
        base_priority,
        vec![AllowedEffect::Reflect],
        format!("satisfied when {title} is resolved"),
        vec![EvidenceRef::try_new("live turn transcript").unwrap()],
        "formed from live discussion".to_string(),
        vec!["test".to_string()],
    )
    .unwrap()
}

/// Runs one formation+detection check (per-turn `live_formation` when `response_dispatch_ref` is
/// `Some`, or the sleep whole-history `sleep_formation` when `None`), resolves it through the
/// same pure coherence-engine resolvers, and writes the `live-goal-formation` trace record.
#[allow(clippy::too_many_arguments)]
fn run_live_formation_check(
    context: &mut RunContext,
    judge: &ScriptedLiveGoalFormationJudge,
    state: VolitionState,
    fixture: &VolitionFixture,
    response_dispatch_ref: &str,
    turn_transcript: &str,
    tick: u64,
    previous_prefix_hash: Option<String>,
    response_dispatched_at: Option<OffsetDateTime>,
    formation_started_at: OffsetDateTime,
) -> anyhow::Result<(VolitionState, Vec<DeclinedCandidate>, String)> {
    let trigger = if response_dispatched_at.is_some() {
        "live_formation"
    } else {
        "sleep_formation"
    };
    let goal_set = coherence_judge_goal_set(&state, fixture);
    let prefix_hash = live_goal_formation_stable_prefix_hash(&goal_set);
    let prefix_cache_eligible = previous_prefix_hash.as_deref() == Some(prefix_hash.as_str());

    let mut invoker = qsf_models::DirectModelInvoker;
    let outcome = judge.form_and_detect(&mut invoker, &goal_set, turn_transcript)?;
    let formation_completed_at = OffsetDateTime::now_utc();

    // Resolution (hard tier-floor gate + admission, or nothing when no candidate was formed)
    // goes through the same `resolve_formed_candidate` the live realtime hook calls, so this
    // harness validates the behavior the live hook actually runs, not a parallel reimplementation.
    let proposed_candidate_json;
    let (events, resolution) = match &outcome.proposed_candidate {
        Some(candidate) => {
            let candidate_tier = effective_tier_from_tension_ids(candidate.tension_ids(), fixture);
            proposed_candidate_json = json!({
                "goal_id": candidate.id(),
                "title": candidate.title(),
                "effective_tier": candidate_tier,
                "tension_ids": candidate.tension_ids(),
            });
            let (events, resolution) =
                resolve_formed_candidate(candidate, &outcome.verdict, &state, fixture, tick);
            (events, Some(resolution))
        }
        None => {
            proposed_candidate_json = serde_json::Value::Null;
            (Vec::new(), None)
        }
    };
    let hard_tier_floor_rejected = resolution
        .as_ref()
        .map(|resolution| matches!(resolution, AdmissionResolution::RejectProtectedFloor));
    let resolution_json = resolution
        .as_ref()
        .map(|resolution| json!(resolution))
        .unwrap_or(serde_json::Value::Null);

    let mut next_state = state.clone();
    for event in events.clone() {
        next_state = apply(next_state, event);
    }
    let newly_declined: Vec<DeclinedCandidate> = outcome
        .proposed_candidate
        .as_ref()
        .and_then(|candidate| newly_declined_candidate(&state, &next_state, candidate.id()))
        .into_iter()
        .collect();
    let (status_before, status_after) = goal_status_diff(&events, &state, &next_state);

    let declined_json = newly_declined.first().map(|declined| json!(declined));

    let details = json!({
        "trigger": trigger,
        "tick": tick,
        "input_ref": response_dispatch_ref,
        "cached_prefix_ref": prefix_hash,
        "prefix_cache_eligible": prefix_cache_eligible,
        "judge_ref": outcome.verdict.judge_ref,
        "proposed_candidate": proposed_candidate_json,
        "goal_set_snapshot": goal_set_snapshot_json(&state, fixture, &goal_set),
        "contradictions": outcome.verdict.contradictions,
        "hard_tier_floor_rejected": hard_tier_floor_rejected,
        "resolution": resolution_json,
        "declined_candidate": declined_json,
        "response_dispatch_ref": response_dispatched_at.map(|_| response_dispatch_ref),
        "response_dispatched_at": response_dispatched_at.map(|t| t.unix_timestamp_nanos()),
        "formation_started_at": formation_started_at.unix_timestamp_nanos(),
        "formation_completed_at": formation_completed_at.unix_timestamp_nanos(),
        "events_emitted": events,
        "goal_status_before": status_before,
        "goal_status_after": status_after,
        "artifact_or_record_reference": format!("live-goal-formation#trigger={trigger}&tick={tick}"),
    });

    let trace_id = write_volition_trace(
        context,
        "live-goal-formation",
        trigger,
        tick,
        &events,
        details,
    )?;
    context.record_event(
        EventType::TraceRecorded,
        json!({
            "trigger": trigger,
            "tick": tick,
            "proposed_candidate": outcome.proposed_candidate.as_ref().map(|c| c.id()),
            "events_emitted": events.len(),
        }),
        Some(trace_id),
    )?;

    Ok((next_state, newly_declined, prefix_hash))
}

fn run_sleep_sweep_check(
    context: &mut RunContext,
    state: VolitionState,
    fixture: &VolitionFixture,
    scripted_pairs: Vec<(String, String, String)>,
    tick: u64,
) -> anyhow::Result<VolitionState> {
    let goal_set = coherence_judge_goal_set(&state, fixture);
    let judge = ScriptedCoherenceJudge::new(scripted_pairs);
    let verdict = judge.judge(context, &goal_set)?;
    let (resolution, events) = resolve_sweep(&verdict, &state, fixture, tick);

    let mut next_state = state.clone();
    for event in events.clone() {
        next_state = apply(next_state, event);
    }
    let (status_before, status_after) = goal_status_diff(&events, &state, &next_state);

    let details = json!({
        "trigger": "sweep",
        "tick": tick,
        "candidate": null,
        "goal_set_snapshot": goal_set_snapshot_json(&state, fixture, &goal_set),
        "judge_ref": verdict.judge_ref,
        "contradictions": verdict.contradictions,
        "hard_tier_floor_rejected": null,
        "resolution": resolution,
        "events_emitted": events,
        "goal_status_before": status_before,
        "goal_status_after": status_after,
        "artifact_or_record_reference": format!("goal-coherence-check#trigger=sweep&tick={tick}"),
    });
    let trace_id = write_coherence_trace(context, "sweep", tick, &events, details)?;
    context.record_event(
        EventType::TraceRecorded,
        json!({
            "trigger": "sweep",
            "tick": tick,
            "cancelled_goal_ids": resolution.cancellations.iter().map(|c| c.cancelled_goal_id.clone()).collect::<Vec<_>>(),
            "flagged_pairs": resolution.flagged.len(),
            "events_emitted": events.len(),
        }),
        Some(trace_id),
    )?;

    Ok(next_state)
}

/// Records the `coherence` context-injection layer's contents for one turn, offline analogue of
/// the realtime `VolitionContextInjected` trace's `declined_candidates_injected` field.
fn write_injection_trace(
    context: &mut RunContext,
    input_transcript_ref: &str,
    tick: u64,
    declined_so_far: &[DeclinedCandidate],
) -> anyhow::Result<Uuid> {
    let declined_candidates_injected: Vec<serde_json::Value> = declined_so_far
        .iter()
        .map(|declined| {
            json!({
                "candidate_id": declined.candidate_id,
                "conflict": declined.conflict,
            })
        })
        .collect();

    let trace_record = TraceRecord::new(
        context.experiment_id(),
        "live-goal-formation-injection",
        format!("tick={tick}"),
        format!("declined_candidates_injected={}", declined_candidates_injected.len()),
    )
    .with_details(json!({
        "input_transcript_ref": input_transcript_ref,
        "tick": tick,
        "injected_layers": [
            { "name": "coherence", "carrier": "conversation.item.create", "injection_point": "after the dynamic volition turn packet and before response.create" }
        ],
        "declined_candidates_injected": declined_candidates_injected,
    }));
    let trace_id = trace_record.trace_id;
    context.record_trace(trace_record)?;
    Ok(trace_id)
}

fn write_live_formation_report(
    context: &RunContext,
    fixture: &VolitionFixture,
    final_state: &VolitionState,
) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Live Goal Formation and Off-Hot-Path Coherence\n\n");
    md.push_str(
        "Offline harness: a scripted formation+detection judge proposes a candidate for each \
         fixture turn (or nothing); pure resolution admits, rejects, or cancels deterministically \
         and reuses the existing goal-lifecycle events. See \
         `docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md`.\n\n",
    );

    md.push_str("## Final Goal Status\n\n");
    md.push_str("| Goal id | Status |\n");
    md.push_str("|---|---|\n");
    let mut goal_ids: Vec<&String> = fixture.goals.iter().map(|goal| &goal.id).collect();
    goal_ids.extend(final_state.accepted_candidates.keys());
    for goal_id in goal_ids {
        if let Some(dynamic) = final_state.goals.get(goal_id) {
            md.push_str(&format!("| `{goal_id}` | {} |\n", dynamic.status));
        }
    }

    md.push_str("\n## Human Verification Checklist\n\n");
    md.push_str(
        "- [ ] Every form/admit/reject/cancel/inject decision is explained by its \
         `live-goal-formation` trace record alone.\n",
    );
    md.push_str(
        "- [ ] The declined candidate is absent from its own rejection turn's injection and \
         present from the next turn onward.\n",
    );
    md.push_str("- [ ] No protected-floor goal was cancelled by the sleep sweep.\n");
    md.push_str(
        "- [ ] `formation_started_at >= response_dispatched_at` holds for every live_formation \
         record.\n",
    );

    fs::write(context.run_dir().join("live-goal-formation-report.md"), md).with_context(|| {
        format!(
            "failed to write live goal formation report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    use serde_json::Value;

    use super::super::registry::{ExperimentName, run_experiment_in};
    use crate::volition::select_goals_ranked;

    #[test]
    fn experiment_runs_and_produces_artifacts() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-live-goal-formation-{}", uuid::Uuid::new_v4()));
        let summary =
            run_experiment_in(&base_dir, ExperimentName::LiveGoalFormationAndCoherence).unwrap();

        assert_eq!(summary.experiment_id, "live-goal-formation-and-coherence");
        assert_eq!(summary.status, "completed");

        let report =
            std::fs::read_to_string(summary.run_dir.join("live-goal-formation-report.md")).unwrap();
        assert!(report.contains("Human Verification Checklist"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn pending_candidate_never_reaches_the_selector() {
        // select_goals_ranked reads only fixture.goals + state.accepted_candidates; a candidate
        // that is only in pending_candidates (added via GoalCandidateAdded, not yet resolved)
        // structurally cannot appear in its output.
        let fixture = super::live_formation_fixture();
        let state = super::VolitionState::from_fixture(&fixture);
        let candidate = super::candidate(
            "just-formed-candidate",
            "Just formed",
            vec!["research-curiosity".to_string()],
            5,
        );
        let state = super::apply(
            state,
            super::VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );

        assert!(
            state
                .pending_candidates
                .iter()
                .any(|c| c.id() == "just-formed-candidate")
        );
        assert!(
            !state
                .accepted_candidates
                .contains_key("just-formed-candidate")
        );

        let ranked = select_goals_ranked("side tangent research", &state, &fixture);
        assert!(
            !ranked
                .selected
                .iter()
                .any(|s| s.goal.id == "just-formed-candidate")
        );
        assert!(
            !ranked
                .omitted
                .iter()
                .any(|o| o.goal.id == "just-formed-candidate")
        );
        assert!(
            !ranked
                .suppressed_cooldown
                .iter()
                .any(|o| o.goal.id == "just-formed-candidate")
        );
        assert!(
            !ranked
                .visible_blocked
                .iter()
                .any(|o| o.goal.id == "just-formed-candidate")
        );
    }

    // ── Trace-completeness contract: parsed from traces.jsonl / events.jsonl on disk. ──

    fn parse_jsonl(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn live_formation_run() -> (Vec<Value>, Vec<Value>, PathBuf) {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-live-goal-formation-{}", uuid::Uuid::new_v4()));
        let summary =
            run_experiment_in(&base_dir, ExperimentName::LiveGoalFormationAndCoherence).unwrap();
        let traces = parse_jsonl(&summary.run_dir.join("traces.jsonl"));
        let events = parse_jsonl(&summary.run_dir.join("events.jsonl"));
        (traces, events, base_dir)
    }

    fn formation_checks(traces: &[Value]) -> Vec<&Value> {
        traces
            .iter()
            .filter(|trace| trace["operation"] == "live-goal-formation")
            .collect()
    }

    fn injection_records(traces: &[Value]) -> Vec<&Value> {
        traces
            .iter()
            .filter(|trace| trace["operation"] == "live-goal-formation-injection")
            .collect()
    }

    fn check_at_tick<'a>(checks: &[&'a Value], tick: u64) -> &'a Value {
        checks
            .iter()
            .copied()
            .find(|trace| trace["details"]["tick"].as_u64() == Some(tick))
            .unwrap_or_else(|| panic!("no live-goal-formation trace at tick {tick}"))
    }

    #[test]
    fn admit_case_emits_accept_event_and_no_decline() {
        let (traces, _events, base_dir) = live_formation_run();
        let checks = formation_checks(&traces);
        let record = check_at_tick(&checks, 2);
        let details = &record["details"];

        assert_eq!(details["trigger"], "live_formation");
        assert!(!details["proposed_candidate"].is_null());
        assert!(details["declined_candidate"].is_null());
        let events_emitted = details["events_emitted"].as_array().unwrap();
        assert!(
            events_emitted
                .iter()
                .any(|event| event["kind"] == "goal_candidate_added")
        );
        assert!(
            events_emitted
                .iter()
                .any(|event| event["kind"] == "goal_candidate_accepted")
        );
        assert!(
            !events_emitted
                .iter()
                .any(|event| event["kind"] == "goal_candidate_rejected")
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn reject_case_emits_rejection_and_a_declined_candidate_record() {
        let (traces, _events, base_dir) = live_formation_run();
        let checks = formation_checks(&traces);
        let record = check_at_tick(&checks, 3);
        let details = &record["details"];

        let events_emitted = details["events_emitted"].as_array().unwrap();
        assert!(
            events_emitted
                .iter()
                .any(|event| event["kind"] == "goal_candidate_rejected")
        );
        assert!(
            !events_emitted
                .iter()
                .any(|event| event["kind"] == "goal_candidate_accepted")
        );
        assert_eq!(
            details["declined_candidate"]["conflict"]["kind"],
            "conflicting_goal"
        );
        assert_eq!(
            details["declined_candidate"]["conflict"]["goal_id"],
            "maintain-coherence"
        );
        assert!(
            details["declined_candidate"]["rationale"]
                .as_str()
                .unwrap()
                .contains("relitigate")
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn declined_candidate_is_absent_from_its_own_turn_and_present_from_the_next() {
        let (traces, _events, base_dir) = live_formation_run();
        let injections = injection_records(&traces);

        let rejection_turn_injection = injections
            .iter()
            .find(|trace| trace["details"]["tick"].as_u64() == Some(3))
            .expect("injection record for the rejection turn itself");
        assert_eq!(
            rejection_turn_injection["details"]["declined_candidates_injected"]
                .as_array()
                .unwrap()
                .len(),
            0,
            "the rejection turn's own context predates admission and must not show its own decline"
        );

        let next_turn_injection = injections
            .iter()
            .find(|trace| trace["details"]["tick"].as_u64() == Some(4))
            .expect("injection record for the turn after the rejection");
        let declined = next_turn_injection["details"]["declined_candidates_injected"]
            .as_array()
            .unwrap();
        assert_eq!(declined.len(), 1);
        assert_eq!(declined[0]["conflict"]["kind"], "conflicting_goal");
        assert_eq!(declined[0]["conflict"]["goal_id"], "maintain-coherence");

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn no_goal_formed_emits_no_lifecycle_events_and_empty_proposed_candidate() {
        let (traces, _events, base_dir) = live_formation_run();
        let checks = formation_checks(&traces);
        let record = check_at_tick(&checks, 4);
        let details = &record["details"];

        assert!(details["proposed_candidate"].is_null());
        assert_eq!(details["events_emitted"].as_array().unwrap().len(), 0);
        assert!(details["declined_candidate"].is_null());

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sleep_whole_history_formation_admits_the_durable_candidate() {
        let (traces, _events, base_dir) = live_formation_run();
        let checks = formation_checks(&traces);
        let record = check_at_tick(&checks, 5);
        let details = &record["details"];

        assert_eq!(details["trigger"], "sleep_formation");
        assert!(details["response_dispatched_at"].is_null());
        assert!(details["response_dispatch_ref"].is_null());
        assert_eq!(
            details["proposed_candidate"]["goal_id"],
            "durable-emerging-priority"
        );
        assert_eq!(details["resolution"]["admitted"], true);

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sleep_sweep_cancels_less_fundamental_drift_goal_never_a_floor_goal() {
        let (traces, _events, base_dir) = live_formation_run();
        let checks: Vec<&Value> = traces
            .iter()
            .filter(|trace| trace["operation"] == "goal-coherence-check")
            .collect();
        let record = check_at_tick(&checks, 8);

        let cancellations = record["details"]["resolution"]["cancellations"]
            .as_array()
            .unwrap();
        assert_eq!(cancellations.len(), 1);
        assert_eq!(
            cancellations[0]["cancelled_goal_id"],
            "sustain-current-focus"
        );
        assert_eq!(cancellations[0]["tie_break"], "greater_activation_tick");

        let events_emitted = record["details"]["events_emitted"].as_array().unwrap();
        assert!(!events_emitted.iter().any(|event| {
            event["kind"] == "goal_retired" && event["goal_id"] == "protect-boundaries"
        }));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn off_hot_path_ordering_holds_for_every_live_formation_record() {
        let (traces, _events, base_dir) = live_formation_run();
        let checks = formation_checks(&traces);

        let mut live_turn_count = 0;
        for record in &checks {
            let details = &record["details"];
            if details["trigger"] != "live_formation" {
                continue;
            }
            live_turn_count += 1;
            let dispatched_at = details["response_dispatched_at"].as_i64().unwrap();
            let started_at = details["formation_started_at"].as_i64().unwrap();
            let completed_at = details["formation_completed_at"].as_i64().unwrap();
            assert!(
                started_at >= dispatched_at,
                "formation_started_at must be >= response_dispatched_at"
            );
            assert!(
                completed_at >= started_at,
                "formation_completed_at must be >= formation_started_at"
            );
        }
        assert_eq!(
            live_turn_count, 3,
            "expected 3 per-turn live_formation records"
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn prefix_cache_eligibility_is_false_exactly_when_the_goal_set_changed() {
        let (traces, _events, base_dir) = live_formation_run();
        let checks = formation_checks(&traces);

        // Tick 2 (admit): first call this run, no prior hash recorded -> not eligible.
        assert_eq!(
            check_at_tick(&checks, 2)["details"]["prefix_cache_eligible"],
            false
        );
        // Tick 3 (reject): tick 2 admitted a candidate, changing the goal set -> not eligible.
        assert_eq!(
            check_at_tick(&checks, 3)["details"]["prefix_cache_eligible"],
            false
        );
        // Tick 4 (no goal formed): tick 3's rejection did not change the goal set -> eligible,
        // and the hash is unchanged from tick 3's.
        assert_eq!(
            check_at_tick(&checks, 4)["details"]["prefix_cache_eligible"],
            true
        );
        assert_eq!(
            check_at_tick(&checks, 3)["details"]["cached_prefix_ref"],
            check_at_tick(&checks, 4)["details"]["cached_prefix_ref"]
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn events_jsonl_links_one_trace_recorded_entry_per_live_goal_formation_check() {
        let (traces, events, base_dir) = live_formation_run();
        let checks = formation_checks(&traces);
        let check_trace_ids: HashSet<String> = checks
            .iter()
            .map(|trace| trace["trace_id"].as_str().unwrap().to_string())
            .collect();

        let trace_recorded: Vec<&Value> = events
            .iter()
            .filter(|event| {
                event["event_type"] == "TraceRecorded"
                    && event["payload"].get("proposed_candidate").is_some()
            })
            .collect();
        assert_eq!(trace_recorded.len(), checks.len());
        for event in &trace_recorded {
            let linked_trace_id = event["trace_id"].as_str().expect("trace_id must be linked");
            assert!(check_trace_ids.contains(linked_trace_id));
        }

        std::fs::remove_dir_all(base_dir).unwrap();
    }
}
