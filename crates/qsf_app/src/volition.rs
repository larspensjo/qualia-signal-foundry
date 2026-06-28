pub use qsf_volition::*;

use serde::{Deserialize, Serialize};

use crate::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextSourceKind, assemble_context,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalSelectionResult {
    pub input: String,
    pub input_terms: Vec<String>,
    pub budget: ContextBudget,
    pub selected: Vec<GoalSelection>,
    pub omitted: Vec<OmittedGoal>,
    pub assembly: ContextAssembly,
}

/// Result of salience-aware goal selection. Adds suppressed and blocked goal lists
/// alongside the standard selected/omitted partitions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SalienceGoalSelectionResult {
    pub input: String,
    pub input_terms: Vec<String>,
    pub budget: ContextBudget,
    pub selected: Vec<GoalSelection>,
    pub omitted: Vec<OmittedGoal>,
    /// Goals suppressed because their runtime status is Cooldown.
    pub suppressed_cooldown: Vec<OmittedGoal>,
    /// Goals kept visible even though they cannot be selected (Blocked status).
    pub visible_blocked: Vec<OmittedGoal>,
    pub assembly: ContextAssembly,
}

pub fn select_goals(
    input: &str,
    fixture: &VolitionFixture,
    budget: ContextBudget,
) -> GoalSelectionResult {
    let input_terms = normalize_terms(input);
    let mut evaluated_fragments = Vec::new();
    let mut omitted = Vec::new();

    for goal in &fixture.goals {
        if goal.status != GoalStatus::Accepted {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {}", goal.status),
            });
            continue;
        }

        let matched_terms = matched_keywords(goal, &input_terms);
        if matched_terms.is_empty() {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms,
                reason: "no activation keywords matched".to_string(),
            });
            continue;
        }

        let relevance_score = compute_relevance(goal, fixture, &matched_terms);
        let fragment = build_fragment(goal, relevance_score, &matched_terms);
        evaluated_fragments.push(GoalEvaluation {
            goal: goal.clone(),
            matched_terms,
            relevance_score,
            fragment,
        });
    }

    let assembly = assemble_context(
        evaluated_fragments
            .iter()
            .map(|evaluation| evaluation.fragment.clone())
            .collect(),
        budget,
    );

    let mut selected = Vec::new();
    for selection in &assembly.selected {
        let evaluation = evaluated_fragments
            .iter()
            .find(|candidate| candidate.fragment.fragment_id == selection.fragment.fragment_id)
            .expect("selected fragment must map back to an evaluated goal");

        selected.push(GoalSelection {
            goal: evaluation.goal.clone(),
            relevance_score: evaluation.relevance_score,
            matched_terms: evaluation.matched_terms.clone(),
            initiative: initiative_for_goal(&evaluation.goal, &evaluation.matched_terms),
        });
    }

    for omission in &assembly.omitted {
        if let Some(evaluation) = evaluated_fragments
            .iter()
            .find(|candidate| candidate.fragment.fragment_id == omission.fragment.fragment_id)
        {
            omitted.push(OmittedGoal {
                goal: evaluation.goal.clone(),
                relevance_score: evaluation.relevance_score,
                matched_terms: evaluation.matched_terms.clone(),
                reason: omission.reason.clone(),
            });
        }
    }

    GoalSelectionResult {
        input: input.to_string(),
        input_terms,
        budget,
        selected,
        omitted,
        assembly,
    }
}

/// Salience-aware selector. Reuses Phase 2 relevance scoring and adds a salience term.
/// Cooldown goals are suppressed; Blocked goals are kept visible but not selected.
/// The existing stateless `select_goals` is unchanged.
pub fn select_goals_with_salience(
    input: &str,
    fixture: &VolitionFixture,
    state: &VolitionState,
    budget: ContextBudget,
) -> SalienceGoalSelectionResult {
    let input_terms = normalize_terms(input);
    let mut evaluated_fragments: Vec<GoalEvaluation> = Vec::new();
    let mut omitted = Vec::new();
    let mut suppressed_cooldown = Vec::new();
    let mut visible_blocked = Vec::new();

    for goal in &fixture.goals {
        let dynamic_status = state
            .goals
            .get(&goal.id)
            .map(|dynamic| dynamic.status)
            .unwrap_or(goal.status);

        // Suppress Cooldown goals entirely.
        if matches!(dynamic_status, GoalStatus::Cooldown) {
            suppressed_cooldown.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {dynamic_status} (cooldown suppressed)"),
            });
            continue;
        }

        // Skip non-selectable statuses (Proposed, Retired).
        if matches!(dynamic_status, GoalStatus::Proposed | GoalStatus::Retired) {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {dynamic_status}"),
            });
            continue;
        }

        let matched_terms = matched_keywords(goal, &input_terms);

        // Blocked goals stay visible but are not selected.
        if matches!(dynamic_status, GoalStatus::Blocked) {
            visible_blocked.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms,
                reason: "goal status is blocked (visible unresolved tension)".to_string(),
            });
            continue;
        }

        if matched_terms.is_empty() {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms,
                reason: "no activation keywords matched".to_string(),
            });
            continue;
        }

        let salience = state
            .goals
            .get(&goal.id)
            .map(|dynamic| dynamic.salience)
            .unwrap_or(0);
        let relevance_score =
            compute_relevance_with_salience(goal, fixture, &matched_terms, salience);
        let fragment = build_fragment(goal, relevance_score, &matched_terms);
        evaluated_fragments.push(GoalEvaluation {
            goal: goal.clone(),
            matched_terms,
            relevance_score,
            fragment,
        });
    }

    // Second pass: accepted candidates use the same logic as fixture goals.
    for goal in state.accepted_candidates.values() {
        let dynamic_status = state
            .goals
            .get(&goal.id)
            .map(|dynamic| dynamic.status)
            .unwrap_or(goal.status);

        if matches!(dynamic_status, GoalStatus::Cooldown) {
            suppressed_cooldown.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {dynamic_status} (cooldown suppressed)"),
            });
            continue;
        }

        if matches!(dynamic_status, GoalStatus::Proposed | GoalStatus::Retired) {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {dynamic_status}"),
            });
            continue;
        }

        let matched_terms = matched_keywords(goal, &input_terms);

        if matches!(dynamic_status, GoalStatus::Blocked) {
            visible_blocked.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms,
                reason: "goal status is blocked (visible unresolved tension)".to_string(),
            });
            continue;
        }

        if matched_terms.is_empty() {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms,
                reason: "no activation keywords matched".to_string(),
            });
            continue;
        }

        let salience = state
            .goals
            .get(&goal.id)
            .map(|dynamic| dynamic.salience)
            .unwrap_or(0);
        let relevance_score =
            compute_relevance_with_salience(goal, fixture, &matched_terms, salience);
        let fragment = build_fragment(goal, relevance_score, &matched_terms);
        evaluated_fragments.push(GoalEvaluation {
            goal: goal.clone(),
            matched_terms,
            relevance_score,
            fragment,
        });
    }

    let assembly = assemble_context(
        evaluated_fragments
            .iter()
            .map(|evaluation| evaluation.fragment.clone())
            .collect(),
        budget,
    );

    let mut selected = Vec::new();
    for selection in &assembly.selected {
        let evaluation = evaluated_fragments
            .iter()
            .find(|candidate| candidate.fragment.fragment_id == selection.fragment.fragment_id)
            .expect("selected fragment must map back to an evaluated goal");

        selected.push(GoalSelection {
            goal: evaluation.goal.clone(),
            relevance_score: evaluation.relevance_score,
            matched_terms: evaluation.matched_terms.clone(),
            initiative: initiative_for_goal(&evaluation.goal, &evaluation.matched_terms),
        });
    }

    for omission in &assembly.omitted {
        if let Some(evaluation) = evaluated_fragments
            .iter()
            .find(|candidate| candidate.fragment.fragment_id == omission.fragment.fragment_id)
        {
            omitted.push(OmittedGoal {
                goal: evaluation.goal.clone(),
                relevance_score: evaluation.relevance_score,
                matched_terms: evaluation.matched_terms.clone(),
                reason: omission.reason.clone(),
            });
        }
    }

    SalienceGoalSelectionResult {
        input: input.to_string(),
        input_terms,
        budget,
        selected,
        omitted,
        suppressed_cooldown,
        visible_blocked,
        assembly,
    }
}

/// Build pre-initiative traces from an already-computed selection result. This is a
/// pure, additive layer over `select_goals`: it records why each selected goal would
/// propose a bounded effect, and an explicit no-delta reason when nothing was selected.
/// It executes no effect and does not change selection behavior.
pub fn build_pre_initiative_traces(
    result: &GoalSelectionResult,
    fixture: &VolitionFixture,
) -> Vec<PreInitiativeTrace> {
    if result.selected.is_empty() {
        return vec![PreInitiativeTrace {
            input: result.input.clone(),
            goal_id: None,
            goal_title: None,
            goal_summary: None,
            tensions: Vec::new(),
            tension_priority_note: TENSION_PRIORITY_NOTE.to_string(),
            delta: DeltaAssessment::NoDelta {
                reason: no_delta_reason(result),
            },
            choice: None,
            allowed_rationale: None,
            executed: false,
        }];
    }

    result
        .selected
        .iter()
        .map(|selection| pre_initiative_trace_for_goal(&result.input, selection, fixture))
        .collect()
}

fn pre_initiative_trace_for_goal(
    input: &str,
    selection: &GoalSelection,
    fixture: &VolitionFixture,
) -> PreInitiativeTrace {
    let goal = &selection.goal;
    let tensions = tension_provenance(goal, fixture);
    let choice = initiative_choice(goal, &selection.matched_terms);
    let allowed_rationale = choice.as_ref().map(|choice| {
        format!(
            "effect '{}' is listed in goal '{}' allowed_effects and is a bounded internal effect (no write-capable external action)",
            choice.proposed.effect, goal.id
        )
    });

    PreInitiativeTrace {
        input: input.to_string(),
        goal_id: Some(goal.id.clone()),
        goal_title: Some(goal.title.clone()),
        goal_summary: Some(goal.summary.clone()),
        tensions,
        tension_priority_note: TENSION_PRIORITY_NOTE.to_string(),
        delta: DeltaAssessment::Delta(DetectedDelta {
            matched_evidence: selection.matched_terms.clone(),
            goal_concern_summary: goal.satisfaction_condition_summary.clone(),
        }),
        choice,
        allowed_rationale,
        executed: false,
    }
}

fn tension_provenance(goal: &Goal, fixture: &VolitionFixture) -> Vec<TensionProvenance> {
    goal.tension_ids
        .iter()
        .filter_map(|tension_id| {
            fixture
                .tensions
                .iter()
                .find(|tension| tension.id == *tension_id)
        })
        .map(|tension| TensionProvenance {
            tension_id: tension.id.clone(),
            title: tension.title.clone(),
            priority_bias: tension.priority_bias,
        })
        .collect()
}

fn initiative_choice(goal: &Goal, matched_terms: &[String]) -> Option<InitiativeChoice> {
    let (chosen_effect, losing_effects) = goal.allowed_effects.split_first()?;
    let proposed = initiative_for_effect(goal, *chosen_effect, matched_terms);

    let losing = losing_effects
        .iter()
        .map(|effect| LosingCandidate {
            proposal: initiative_for_effect(goal, *effect, matched_terms),
            reason: format!(
                "not selected: goal '{}' orders '{}' after the chosen effect '{}' in allowed_effects precedence",
                goal.id, effect, chosen_effect
            ),
        })
        .collect();

    Some(InitiativeChoice { proposed, losing })
}

fn no_delta_reason(result: &GoalSelectionResult) -> String {
    let mut reasons: Vec<String> = Vec::new();
    for omitted in &result.omitted {
        if !reasons.iter().any(|reason| reason == &omitted.reason) {
            reasons.push(omitted.reason.clone());
        }
    }

    if reasons.is_empty() {
        "no goal was selected and no goals were available to omit".to_string()
    } else {
        format!(
            "no goal selected; the input carries no tracked volition delta (omitted goals: {})",
            reasons.join("; ")
        )
    }
}

fn initiative_for_goal(goal: &Goal, matched_terms: &[String]) -> InitiativeProposal {
    let effect = goal
        .allowed_effects
        .first()
        .copied()
        .unwrap_or(AllowedEffect::Reflect);

    initiative_for_effect(goal, effect, matched_terms)
}

fn initiative_for_effect(
    goal: &Goal,
    effect: AllowedEffect,
    matched_terms: &[String],
) -> InitiativeProposal {
    InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect,
        rationale: format!(
            "goal {} matched [{}] under scope {}",
            goal.id,
            matched_terms.join(", "),
            goal.scope
        ),
        matched_terms: matched_terms.to_vec(),
        scope: goal.scope,
    }
}

fn build_fragment(goal: &Goal, relevance_score: f64, matched_terms: &[String]) -> ContextFragment {
    let mut tags = goal.activation_keywords.clone();
    tags.extend(goal.tension_ids.iter().cloned());
    tags.push(goal.scope.to_string());

    ContextFragment {
        fragment_id: goal.id.clone(),
        source_kind: ContextSourceKind::RuntimeState,
        summary: goal.summary.clone(),
        tags,
        score: relevance_score,
        estimated_tokens: goal.estimated_tokens,
        source_reference: goal.source_reference.clone(),
        selection_reason: format!(
            "matched keywords: {}; tensions: {}; scope: {}",
            matched_terms.join(", "),
            goal.tension_ids.join(", "),
            goal.scope
        ),
    }
}

fn compute_relevance_with_salience(
    goal: &Goal,
    fixture: &VolitionFixture,
    matched_terms: &[String],
    salience: i32,
) -> f64 {
    compute_relevance(goal, fixture, matched_terms) + salience as f64
}

fn compute_relevance(goal: &Goal, fixture: &VolitionFixture, matched_terms: &[String]) -> f64 {
    let matched_bonus = matched_terms.len() as f64 * 100.0;
    let base_priority = goal.base_priority as f64;
    let tension_bonus = goal
        .tension_ids
        .iter()
        .filter_map(|tension_id| {
            fixture
                .tensions
                .iter()
                .find(|tension| tension.id == *tension_id)
        })
        .map(|tension| tension.priority_bias.score_bonus())
        .fold(0.0, f64::max);

    matched_bonus + base_priority + tension_bonus
}

fn matched_keywords(goal: &Goal, input_terms: &[String]) -> Vec<String> {
    let mut matched = Vec::new();

    for keyword in &goal.activation_keywords {
        if input_terms.iter().any(|term| term == keyword)
            && !matched.iter().any(|term| term == keyword)
        {
            matched.push(keyword.clone());
        }
    }

    matched
}

#[derive(Clone)]
struct GoalEvaluation {
    goal: Goal,
    matched_terms: Vec<String>,
    relevance_score: f64,
    fragment: ContextFragment,
}

#[cfg(test)]
mod tests {
    use super::{
        COOLDOWN_SPAN_TICKS, DeltaAssessment, EvidenceRef, GoalSelectionResult, GoalStatus,
        RETIREMENT_INACTIVITY_TICKS, SALIENCE_ACTIVATION_BONUS, SALIENCE_DECAY_PER_TICK,
        VolitionEvent, VolitionState, apply, build_pre_initiative_traces, select_goals,
        static_fixture, tick_events,
    };
    use crate::context::ContextBudget;

    // ── EvidenceRef validation ──────────────────────────────────────────────

    #[test]
    fn evidence_ref_rejects_empty_string() {
        assert!(EvidenceRef::try_new("").is_err());
    }

    #[test]
    fn evidence_ref_rejects_whitespace_only() {
        assert!(EvidenceRef::try_new("   ").is_err());
        assert!(EvidenceRef::try_new("\t\n").is_err());
    }

    #[test]
    fn evidence_ref_accepts_non_empty() {
        let r = EvidenceRef::try_new("docs/Experiment.md").unwrap();
        assert_eq!(r.as_str(), "docs/Experiment.md");
    }

    #[test]
    fn evidence_ref_try_from_string_works() {
        let r = EvidenceRef::try_from("trace-42".to_string()).unwrap();
        assert_eq!(r.as_str(), "trace-42");
    }

    // ── GoalActivated ───────────────────────────────────────────────────────

    #[test]
    fn goal_activated_sets_active_and_raises_salience() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );

        let dynamic = state.goal(goal_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Active);
        assert_eq!(dynamic.salience, SALIENCE_ACTIVATION_BONUS);
        assert_eq!(dynamic.last_activated_tick, Some(1));
    }

    #[test]
    fn repeated_activations_raise_salience_monotonically_before_decay() {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let mut prev_salience = 0;
        for tick in 1..=5 {
            state = apply(
                state,
                VolitionEvent::GoalActivated {
                    goal_id: goal_id.to_string(),
                    tick,
                },
            );
            let s = state.goal(goal_id).unwrap().salience;
            assert!(
                s > prev_salience,
                "salience should rise monotonically, tick={tick}"
            );
            prev_salience = s;
        }
    }

    #[test]
    fn irrelevant_goal_stays_at_zero_salience() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let activated_id = "clarify-weak-evidence-topic";
        let other_id = "avoid-overstating-impl-status";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: activated_id.to_string(),
                tick: 1,
            },
        );

        assert_eq!(state.goal(other_id).unwrap().salience, 0);
    }

    // ── GoalProgressObserved ────────────────────────────────────────────────

    #[test]
    fn progress_appends_evidence_ref_and_increments_reinforcement() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let evidence = EvidenceRef::try_new("trace-42").unwrap();

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalProgressObserved {
                goal_id: goal_id.to_string(),
                evidence: evidence.clone(),
                tick: 2,
            },
        );

        let dynamic = state.goal(goal_id).unwrap();
        assert_eq!(dynamic.reinforcement_count, 1);
        assert!(dynamic.progress_evidence_refs.contains(&evidence));
        assert!(dynamic.salience > SALIENCE_ACTIVATION_BONUS);
    }

    // ── GoalDecayed ─────────────────────────────────────────────────────────

    #[test]
    fn decay_lowers_salience_by_deterministic_amount() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let salience_before = state.goal(goal_id).unwrap().salience;

        let state = apply(
            state,
            VolitionEvent::GoalDecayed {
                goal_id: goal_id.to_string(),
                tick: 2,
            },
        );

        let salience_after = state.goal(goal_id).unwrap().salience;
        assert_eq!(salience_before - salience_after, SALIENCE_DECAY_PER_TICK);
        assert_eq!(
            state.goal(goal_id).unwrap().status,
            GoalStatus::Active,
            "decay must not change status"
        );
    }

    #[test]
    fn decay_does_not_go_below_zero() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let mut state = state;
        for tick in 2..=20 {
            state = apply(
                state,
                VolitionEvent::GoalDecayed {
                    goal_id: goal_id.to_string(),
                    tick,
                },
            );
        }

        assert_eq!(state.goal(goal_id).unwrap().salience, 0);
    }

    // ── GoalSatisfied + GoalCooldownElapsed ─────────────────────────────────

    #[test]
    fn satisfaction_enters_cooldown_and_resets_salience() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let evidence = EvidenceRef::try_new("docs/trace.md").unwrap();

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: goal_id.to_string(),
                evidence,
                tick: 2,
            },
        );

        let dynamic = state.goal(goal_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Cooldown);
        assert_eq!(dynamic.salience, 0);
        assert_eq!(dynamic.last_satisfied_tick, Some(2));
        assert_eq!(dynamic.cooldown_until_tick, Some(2 + COOLDOWN_SPAN_TICKS));
    }

    #[test]
    fn cooldown_elapsed_returns_goal_to_accepted() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let evidence = EvidenceRef::try_new("docs/trace.md").unwrap();

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: goal_id.to_string(),
                evidence,
                tick: 2,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalCooldownElapsed {
                goal_id: goal_id.to_string(),
                tick: 2 + COOLDOWN_SPAN_TICKS,
            },
        );

        let dynamic = state.goal(goal_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Accepted);
        assert!(dynamic.cooldown_until_tick.is_none());
    }

    // ── GoalBlocked ─────────────────────────────────────────────────────────

    #[test]
    fn blocked_goal_keeps_status_and_nonzero_salience() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let salience_before = state.goal(goal_id).unwrap().salience;
        let state = apply(
            state,
            VolitionEvent::GoalBlocked {
                goal_id: goal_id.to_string(),
                tick: 2,
            },
        );

        let dynamic = state.goal(goal_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Blocked);
        assert_eq!(
            dynamic.salience, salience_before,
            "blocked must preserve salience"
        );
    }

    // ── GoalRetired ──────────────────────────────────────────────────────────

    #[test]
    fn retired_goal_reaches_retired_status() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalRetired {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );

        assert_eq!(state.goal(goal_id).unwrap().status, GoalStatus::Retired);
    }

    // ── tick_events ──────────────────────────────────────────────────────────

    #[test]
    fn tick_events_emits_decay_for_active_goal_with_salience() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let events = tick_events(&state, &fixture, 2);

        assert!(events.iter().any(|event| matches!(
            event,
            VolitionEvent::GoalDecayed { goal_id: id, .. } if id == "clarify-weak-evidence-topic"
        )));
    }

    #[test]
    fn tick_events_emits_retirement_for_zero_salience_inactive_goal() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let events = tick_events(&state, &fixture, RETIREMENT_INACTIVITY_TICKS);

        assert!(events.iter().any(|event| matches!(
            event,
            VolitionEvent::GoalRetired { goal_id: id, .. } if id == goal_id
        )));
    }

    #[test]
    fn tick_events_emits_cooldown_elapsed_after_span() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let evidence = EvidenceRef::try_new("docs/trace.md").unwrap();

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: goal_id.to_string(),
                evidence,
                tick: 2,
            },
        );

        let events = tick_events(&state, &fixture, 2 + COOLDOWN_SPAN_TICKS);

        assert!(events.iter().any(|event| matches!(
            event,
            VolitionEvent::GoalCooldownElapsed { goal_id: id, .. } if id == goal_id
        )));
    }

    // ── select_goals_with_salience ───────────────────────────────────────────

    #[test]
    fn salience_aware_selector_matches_stateless_when_state_is_empty() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "We never settled how voice memory affects continuity.";
        let budget = ContextBudget::new(2, 80);

        let stateless = select_goals(input, &fixture, budget);
        let salience_result = super::select_goals_with_salience(input, &fixture, &state, budget);

        let stateless_ids: Vec<_> = stateless.selected.iter().map(|s| &s.goal.id).collect();
        let salience_ids: Vec<_> = salience_result
            .selected
            .iter()
            .map(|s| &s.goal.id)
            .collect();
        assert_eq!(
            stateless_ids, salience_ids,
            "empty state must not alter selection"
        );
    }

    #[test]
    fn cooldown_goal_is_suppressed_from_selection() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let evidence = EvidenceRef::try_new("docs/trace.md").unwrap();

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: goal_id.to_string(),
                evidence,
                tick: 2,
            },
        );

        let input = "We never settled how voice memory affects continuity.";
        let result =
            super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(2, 80));

        assert!(
            result.selected.iter().all(|s| s.goal.id != goal_id),
            "cooldown goal must not appear in selected"
        );
        assert!(
            result
                .suppressed_cooldown
                .iter()
                .any(|s| s.goal.id == goal_id),
            "cooldown goal must appear in suppressed_cooldown"
        );
    }

    #[test]
    fn blocked_goal_stays_visible_but_not_selected() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalBlocked {
                goal_id: goal_id.to_string(),
                tick: 2,
            },
        );

        let input = "We never settled how voice memory affects continuity.";
        let result =
            super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(2, 80));

        assert!(
            result.selected.iter().all(|s| s.goal.id != goal_id),
            "blocked goal must not appear in selected"
        );
        assert!(
            result.visible_blocked.iter().any(|s| s.goal.id == goal_id),
            "blocked goal must stay visible in visible_blocked"
        );
        assert!(
            result.visible_blocked.iter().all(|s| !s.reason.is_empty()),
            "blocked goal must carry a reason"
        );
    }

    // ── Tick monotonicity ────────────────────────────────────────────────────

    #[test]
    fn reducer_tick_never_decreases_on_lower_tick_event() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 5,
            },
        );
        assert_eq!(state.tick, 5);

        let state = apply(
            state,
            VolitionEvent::GoalDecayed {
                goal_id: goal_id.to_string(),
                tick: 3,
            },
        );
        assert_eq!(state.tick, 5, "lower-tick event must not regress tick");
    }

    #[test]
    fn reducer_tick_is_stable_across_same_tick_events() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 4,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalDecayed {
                goal_id: goal_id.to_string(),
                tick: 4,
            },
        );
        assert_eq!(state.tick, 4, "duplicate-tick event must not move tick");
    }

    // ── Replay determinism ───────────────────────────────────────────────────

    #[test]
    fn same_event_sequence_yields_identical_state() {
        let fixture = static_fixture();
        let evidence = EvidenceRef::try_new("docs/trace.md").unwrap();

        let run = || {
            let state = VolitionState::from_fixture(&fixture);
            let goal_id = "clarify-weak-evidence-topic";
            let state = apply(
                state,
                VolitionEvent::GoalActivated {
                    goal_id: goal_id.to_string(),
                    tick: 1,
                },
            );
            let state = apply(
                state,
                VolitionEvent::GoalProgressObserved {
                    goal_id: goal_id.to_string(),
                    evidence: evidence.clone(),
                    tick: 2,
                },
            );
            apply(
                state,
                VolitionEvent::GoalBlocked {
                    goal_id: goal_id.to_string(),
                    tick: 3,
                },
            )
        };

        assert_eq!(run(), run());
    }

    #[test]
    fn baseline_input_selects_no_goals() {
        let fixture = static_fixture();
        let result = select_goals(
            "Give me the build command.",
            &fixture,
            ContextBudget::new(2, 80),
        );

        assert!(result.selected.is_empty());
        assert!(
            result
                .omitted
                .iter()
                .all(|omitted| omitted.reason == "no activation keywords matched")
        );
    }

    #[test]
    fn selection_is_deterministic_for_the_same_input() {
        let fixture = static_fixture();
        let first = select_goals(
            "We never settled how voice memory affects continuity.",
            &fixture,
            ContextBudget::new(2, 80),
        );
        let second = select_goals(
            "We never settled how voice memory affects continuity.",
            &fixture,
            ContextBudget::new(2, 80),
        );

        assert_eq!(first, second);
    }

    #[test]
    fn token_budget_limits_selected_goals() {
        let fixture = static_fixture();
        let result = select_goals(
            "We never settled how voice memory affects continuity.",
            &fixture,
            ContextBudget::new(2, 40),
        );

        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].goal.id, "clarify-weak-evidence-topic");
        assert!(result.assembly.used_estimated_tokens <= 40);
        assert!(
            result
                .omitted
                .iter()
                .any(|omitted| omitted.goal.id == "resurface-open-thread")
        );
    }

    #[test]
    fn perturbing_the_fixture_changes_selection_predictably() {
        let fixture = static_fixture();
        let base = select_goals(
            "We never settled how voice memory affects continuity.",
            &fixture,
            ContextBudget::new(2, 80),
        );

        let mut perturbed = fixture.clone();
        let goal = perturbed
            .goals
            .iter_mut()
            .find(|goal| goal.id == "resurface-open-thread")
            .unwrap();
        goal.activation_keywords
            .retain(|keyword| keyword != "continuity");

        let changed = select_goals(
            "We never settled how voice memory affects continuity.",
            &perturbed,
            ContextBudget::new(2, 80),
        );

        assert!(
            base.selected
                .iter()
                .any(|selection| selection.goal.id == "resurface-open-thread")
        );
        assert!(
            !changed
                .selected
                .iter()
                .any(|selection| selection.goal.id == "resurface-open-thread")
        );
        assert!(
            changed
                .selected
                .iter()
                .any(|selection| selection.goal.id == "clarify-weak-evidence-topic")
        );
    }

    #[test]
    fn goal_selection_result_serializes() {
        let fixture = static_fixture();
        let result: GoalSelectionResult = select_goals(
            "Is the goal system implemented yet?",
            &fixture,
            ContextBudget::new(2, 80),
        );

        let json = serde_json::to_value(result).unwrap();

        assert_eq!(
            json["selected"][0]["goal"]["id"],
            "avoid-overstating-impl-status"
        );
    }

    #[test]
    fn selected_goal_trace_records_delta_tensions_and_choice() {
        let fixture = static_fixture();
        let result = select_goals(
            "We never settled how voice memory affects continuity.",
            &fixture,
            ContextBudget::new(2, 80),
        );
        let traces = build_pre_initiative_traces(&result, &fixture);

        let trace = traces
            .iter()
            .find(|trace| trace.goal_id.as_deref() == Some("clarify-weak-evidence-topic"))
            .expect("continuity input should trace the weak-evidence goal");

        match &trace.delta {
            DeltaAssessment::Delta(delta) => {
                assert!(!delta.matched_evidence.is_empty());
                assert!(!delta.goal_concern_summary.is_empty());
            }
            DeltaAssessment::NoDelta { reason } => {
                panic!("expected a delta, got no-delta reason: {reason}")
            }
        }

        assert!(
            !trace.tensions.is_empty(),
            "selected goal should record tension provenance"
        );

        assert_eq!(
            trace.goal_summary.as_deref(),
            Some(
                "Surface a research question when the input points at uncertain or under-explained material."
            ),
            "selected-goal trace should be self-contained with the goal summary"
        );

        let choice = trace
            .choice
            .as_ref()
            .expect("selected goal proposes an effect");
        assert_eq!(choice.proposed.effect.to_string(), "reflect");
        assert_eq!(choice.losing.len(), 1);
        assert_eq!(
            choice.losing[0].proposal.effect.to_string(),
            "propose-experiment"
        );
        assert!(!choice.losing[0].reason.is_empty());
    }

    #[test]
    fn baseline_input_produces_single_no_delta_trace() {
        let fixture = static_fixture();
        let result = select_goals(
            "Give me the build command.",
            &fixture,
            ContextBudget::new(2, 80),
        );
        let traces = build_pre_initiative_traces(&result, &fixture);

        assert_eq!(traces.len(), 1);
        let trace = &traces[0];
        assert!(trace.goal_id.is_none());
        assert!(trace.goal_summary.is_none());
        assert!(trace.choice.is_none());
        assert!(matches!(trace.delta, DeltaAssessment::NoDelta { .. }));
    }

    #[test]
    fn traces_never_execute_an_effect() {
        let fixture = static_fixture();
        for input in [
            "We never settled how voice memory affects continuity.",
            "Give me the build command.",
            "Is the goal system implemented yet?",
        ] {
            let result = select_goals(input, &fixture, ContextBudget::new(2, 80));
            let traces = build_pre_initiative_traces(&result, &fixture);
            assert!(traces.iter().all(|trace| !trace.executed));
        }
    }

    #[test]
    fn traces_are_deterministic_for_the_same_input() {
        let fixture = static_fixture();
        let input = "We never settled how voice memory affects continuity.";
        let first = build_pre_initiative_traces(
            &select_goals(input, &fixture, ContextBudget::new(2, 80)),
            &fixture,
        );
        let second = build_pre_initiative_traces(
            &select_goals(input, &fixture, ContextBudget::new(2, 80)),
            &fixture,
        );

        assert_eq!(first, second);
    }

    #[test]
    fn every_selected_goal_trace_carries_a_proposed_effect() {
        let fixture = static_fixture();
        let result = select_goals(
            "We never settled how voice memory affects continuity.",
            &fixture,
            ContextBudget::new(2, 80),
        );
        let traces = build_pre_initiative_traces(&result, &fixture);

        assert!(!traces.is_empty());
        for trace in &traces {
            assert!(trace.goal_id.is_some());
            assert!(matches!(trace.delta, DeltaAssessment::Delta(_)));
            assert!(trace.choice.is_some());
            assert!(trace.allowed_rationale.is_some());
        }
    }

    #[test]
    fn serialized_traces_are_deterministic_for_the_full_scripted_set() {
        let fixture = static_fixture();
        let scripted_inputs = [
            "Is the goal system implemented yet?",
            "We never settled how voice memory affects continuity.",
            "Give me the build command.",
            "Should we turn the volition note into a tiny experiment?",
        ];

        let serialize_all = || {
            let mut serialized = String::new();
            for input in scripted_inputs {
                let result = select_goals(input, &fixture, ContextBudget::new(2, 80));
                for trace in build_pre_initiative_traces(&result, &fixture) {
                    serialized.push_str(&serde_json::to_string(&trace).unwrap());
                    serialized.push('\n');
                }
            }
            serialized
        };

        assert_eq!(serialize_all(), serialize_all());
    }

    // ── arbitrate() ─────────────────────────────────────────────────────────

    use super::{
        AllowedEffect, Goal, GoalScope, GoalSelection, InitiativeProposal, Tension,
        TensionPriority, VolitionFixture, arbitrate,
    };

    fn make_goal_for_arbitration(
        id: &str,
        tension_ids: Vec<String>,
        base_priority: u8,
    ) -> GoalSelection {
        let goal = Goal {
            id: id.to_string(),
            title: id.to_string(),
            summary: id.to_string(),
            tension_ids,
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority,
            activation_keywords: vec!["test".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: id.to_string(),
            evidence_refs: vec![],
            estimated_tokens: 10,
            source_reference: id.to_string(),
        };
        GoalSelection {
            goal: goal.clone(),
            relevance_score: goal.base_priority as f64,
            matched_terms: vec!["test".to_string()],
            initiative: InitiativeProposal {
                goal_id: goal.id.clone(),
                goal_title: goal.title.clone(),
                effect: AllowedEffect::Reflect,
                rationale: "test".to_string(),
                matched_terms: vec!["test".to_string()],
                scope: GoalScope::Session,
            },
        }
    }

    fn make_tension(id: &str, tier: u8) -> Tension {
        Tension {
            id: id.to_string(),
            title: format!("{id} title"),
            summary: "test".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: tier,
        }
    }

    #[test]
    fn arbitrate_empty_returns_none() {
        let fixture = static_fixture();
        assert!(arbitrate(vec![], &fixture).is_none());
    }

    #[test]
    fn arbitrate_single_selection_is_winner_with_no_losers() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let result = super::select_goals_with_salience(
            "Is the implementation status complete?",
            &fixture,
            &state,
            ContextBudget::new(2, 80),
        );
        // Only avoid-overstating-impl-status matches (keywords: status, complete)
        assert_eq!(result.selected.len(), 1);
        let arbitration = arbitrate(result.selected.clone(), &fixture).unwrap();
        assert_eq!(arbitration.winner.goal.id, "avoid-overstating-impl-status");
        assert!(arbitration.losers.is_empty());
    }

    #[test]
    fn arbitrate_lower_tier_wins_over_higher_tier() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        // "status"/"complete" → avoid-overstating-impl-status (tier 1 via boundary-preservation)
        // "continuity"/"thread" → resurface-open-thread (tier 5 via continuity-preservation)
        let result = super::select_goals_with_salience(
            "Is the implementation status complete in this continuity thread?",
            &fixture,
            &state,
            ContextBudget::new(4, 100),
        );
        assert_eq!(result.selected.len(), 2, "expected 2 selected goals");

        let arbitration = arbitrate(result.selected.clone(), &fixture).unwrap();
        assert_eq!(arbitration.winner.goal.id, "avoid-overstating-impl-status");
        assert_eq!(arbitration.winner_effective_tier, 1);
        assert_eq!(
            arbitration.winner_effective_tension_id,
            "boundary-preservation"
        );
        assert_eq!(
            arbitration.winner_effective_tension_title,
            "Boundary preservation"
        );
        assert_eq!(arbitration.losers.len(), 1);
        assert_eq!(
            arbitration.losers[0].selection.goal.id,
            "resurface-open-thread"
        );
        assert_eq!(arbitration.losers[0].effective_tier, 5);
        assert_eq!(
            arbitration.losers[0].effective_tension_id,
            "continuity-preservation"
        );
    }

    #[test]
    fn arbitrate_same_tier_higher_base_priority_wins() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        // "voice"/"memory"/"evidence"/"unclear" → clarify-weak-evidence-topic (tier 7, priority 85)
        // "experiment" → propose-followup-experiment (tier 7, priority 90)
        let result = super::select_goals_with_salience(
            "The voice memory experiment evidence is unclear.",
            &fixture,
            &state,
            ContextBudget::new(4, 100),
        );
        // Both goals are at tier 7 via research-curiosity
        let selected_ids: Vec<&str> = result.selected.iter().map(|s| s.goal.id.as_str()).collect();
        assert!(
            selected_ids.contains(&"clarify-weak-evidence-topic"),
            "selected: {selected_ids:?}"
        );
        assert!(
            selected_ids.contains(&"propose-followup-experiment"),
            "selected: {selected_ids:?}"
        );

        let arbitration = arbitrate(result.selected.clone(), &fixture).unwrap();
        // propose-followup-experiment (priority 90) beats clarify-weak-evidence-topic (priority 85)
        assert_eq!(arbitration.winner.goal.id, "propose-followup-experiment");
        assert_eq!(arbitration.winner_effective_tier, 7);
        assert_eq!(
            arbitration.winner_effective_tension_id,
            "research-curiosity"
        );
        assert_eq!(arbitration.losers.len(), 1);
        assert_eq!(
            arbitration.losers[0].selection.goal.id,
            "clarify-weak-evidence-topic"
        );
        assert_eq!(arbitration.losers[0].effective_tier, 7);
    }

    #[test]
    fn arbitrate_same_tier_same_priority_lower_goal_id_wins() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("test-tension", 5)],
            goals: vec![],
        };
        // "goal-a" < "goal-b" lexicographically; same tier and priority
        let sel_b = make_goal_for_arbitration("goal-b", vec!["test-tension".to_string()], 80);
        let sel_a = make_goal_for_arbitration("goal-a", vec!["test-tension".to_string()], 80);
        let result = arbitrate(vec![sel_b, sel_a], &fixture).unwrap();
        assert_eq!(result.winner.goal.id, "goal-a");
        assert_eq!(result.losers[0].selection.goal.id, "goal-b");
    }

    #[test]
    fn arbitrate_multi_tension_goal_uses_minimum_tier() {
        // avoid-overstating-impl-status has coherence-maintenance (tier 4) AND
        // boundary-preservation (tier 1). Effective tier must be 1 (the minimum).
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let result = super::select_goals_with_salience(
            "Is the implementation status complete?",
            &fixture,
            &state,
            ContextBudget::new(2, 80),
        );
        assert_eq!(result.selected.len(), 1);
        let arbitration = arbitrate(result.selected, &fixture).unwrap();
        assert_eq!(
            arbitration.winner_effective_tier, 1,
            "effective tier must be the minimum among parent tensions"
        );
        assert_eq!(
            arbitration.winner_effective_tension_id, "boundary-preservation",
            "effective tension is the one at the minimum tier"
        );
    }

    #[test]
    fn arbitrate_same_minimum_tier_picks_lexicographic_tension_id() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("beta-tension", 3),
                make_tension("alpha-tension", 3),
            ],
            goals: vec![],
        };
        // Goal backed by both tensions at tier 3; alpha < beta lexicographically
        let sel = make_goal_for_arbitration(
            "test-goal",
            vec!["alpha-tension".to_string(), "beta-tension".to_string()],
            80,
        );
        let result = arbitrate(vec![sel], &fixture).unwrap();
        assert_eq!(result.winner_effective_tier, 3);
        assert_eq!(result.winner_effective_tension_id, "alpha-tension");
        assert_eq!(result.winner_effective_tension_title, "alpha-tension title");
    }

    #[test]
    fn arbitrate_losers_are_sorted_by_tier_then_priority_then_id() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("tier-1-tension", 1),
                make_tension("tier-5-tension", 5),
                make_tension("tier-7-tension", 7),
            ],
            goals: vec![],
        };
        let sel_tier7 =
            make_goal_for_arbitration("goal-z-tier7", vec!["tier-7-tension".to_string()], 80);
        let sel_tier5 =
            make_goal_for_arbitration("goal-a-tier5", vec!["tier-5-tension".to_string()], 90);
        let sel_tier1 =
            make_goal_for_arbitration("goal-m-tier1", vec!["tier-1-tension".to_string()], 95);
        let result = arbitrate(vec![sel_tier7, sel_tier5, sel_tier1], &fixture).unwrap();

        assert_eq!(result.winner.goal.id, "goal-m-tier1");
        assert_eq!(result.winner_effective_tier, 1);
        // Losers: tier 5 before tier 7 (ascending tier)
        assert_eq!(result.losers[0].selection.goal.id, "goal-a-tier5");
        assert_eq!(result.losers[0].effective_tier, 5);
        assert_eq!(result.losers[1].selection.goal.id, "goal-z-tier7");
        assert_eq!(result.losers[1].effective_tier, 7);
    }

    #[test]
    fn arbitrate_result_is_deterministic() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "Is the continuity thread complete enough to be confident in the evidence?";
        let budget = ContextBudget::new(4, 100);

        let run = || {
            let result = super::select_goals_with_salience(input, &fixture, &state, budget);
            arbitrate(result.selected, &fixture)
        };

        assert_eq!(run(), run());
    }

    #[test]
    fn arbitrate_no_effect_is_executed() {
        // arbitrate() is a pure function that returns data only; the ArbitrationResult
        // carries no executed flag because execution is structurally impossible.
        // Verify that the initiative proposals in the result carry the expected effect
        // but nothing in the result signals actual execution.
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let result = super::select_goals_with_salience(
            "Is the implementation status complete in this continuity thread?",
            &fixture,
            &state,
            ContextBudget::new(4, 100),
        );
        let arbitration = arbitrate(result.selected, &fixture).unwrap();
        // The winner carries an initiative proposal (not executed), losers likewise.
        // This assertion documents the contract: arbitrate() proposes, never executes.
        assert!(!arbitration.winner.initiative.goal_id.is_empty());
        for loser in &arbitration.losers {
            assert!(!loser.selection.initiative.goal_id.is_empty());
        }
    }

    // ── Phase 6: ProposedGoalCandidate ─────────────────────────────────────────

    use super::ProposedGoalCandidate;

    fn make_candidate(id: &str) -> ProposedGoalCandidate {
        let evidence = EvidenceRef::try_new(format!("open-question: {id}")).unwrap();
        ProposedGoalCandidate::try_new(
            id.to_string(),
            format!("Title {id}"),
            format!("Summary for {id}"),
            vec![],
            GoalScope::Session,
            70,
            vec![AllowedEffect::Reflect],
            "Satisfied when resolved.".to_string(),
            vec![evidence],
            format!("source: {id}"),
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn proposed_goal_candidate_rejects_empty_evidence() {
        let result = ProposedGoalCandidate::try_new(
            "test-id".to_string(),
            "Test".to_string(),
            "Summary".to_string(),
            vec![],
            GoalScope::Session,
            80,
            vec![AllowedEffect::Reflect],
            "Satisfied when done.".to_string(),
            vec![],
            "open-question: test".to_string(),
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn proposed_goal_candidate_accepts_valid_evidence() {
        let evidence = EvidenceRef::try_new("open-question: test question").unwrap();
        let result = ProposedGoalCandidate::try_new(
            "test-id".to_string(),
            "Test".to_string(),
            "Summary".to_string(),
            vec![],
            GoalScope::Session,
            80,
            vec![AllowedEffect::Reflect],
            "Satisfied when done.".to_string(),
            vec![evidence],
            "open-question: test question".to_string(),
            vec![],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn goal_candidate_added_appends_to_pending_candidates() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-1");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );

        assert_eq!(state.pending_candidates.len(), 1);
        assert_eq!(state.pending_candidates[0].id(), "cand-1");
    }

    #[test]
    fn goal_candidate_added_does_not_auto_accept() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-1");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );

        assert!(!state.accepted_candidates.contains_key("cand-1"));
    }

    #[test]
    fn goal_candidate_accepted_moves_candidate_to_accepted() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-accept");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let acceptance_evidence = EvidenceRef::try_new("experiment: confirmed useful").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "cand-accept".to_string(),
                acceptance_evidence,
                tick: 2,
            },
        );

        assert!(
            !state
                .pending_candidates
                .iter()
                .any(|c| c.id() == "cand-accept")
        );
        assert!(state.accepted_candidates.contains_key("cand-accept"));
    }

    #[test]
    fn goal_candidate_accepted_without_prior_add_is_noop() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let acceptance_evidence = EvidenceRef::try_new("experiment: confirmed").unwrap();

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "nonexistent".to_string(),
                acceptance_evidence,
                tick: 1,
            },
        );

        assert!(!state.accepted_candidates.contains_key("nonexistent"));
    }

    #[test]
    fn goal_candidate_rejected_removes_from_pending() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-reject");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let state = apply(
            state,
            VolitionEvent::GoalCandidateRejected {
                goal_id: "cand-reject".to_string(),
                reason: "Not relevant enough.".to_string(),
                tick: 2,
            },
        );

        assert!(
            !state
                .pending_candidates
                .iter()
                .any(|c| c.id() == "cand-reject")
        );
        assert!(!state.accepted_candidates.contains_key("cand-reject"));
    }

    #[test]
    fn remaining_candidate_stays_in_pending_across_tick() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-stay");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let state = apply(state, VolitionEvent::TickAdvanced { tick: 2 });

        assert_eq!(
            state
                .pending_candidates
                .iter()
                .filter(|c| c.id() == "cand-stay")
                .count(),
            1
        );
        assert!(!state.accepted_candidates.contains_key("cand-stay"));
    }

    #[test]
    fn accepted_candidate_goal_data_in_accepted_candidates_dynamic_state_in_goals() {
        // Goal data (the Goal struct) lives in accepted_candidates.
        // Dynamic state (GoalDynamicState) lives in state.goals — same map as fixture
        // goals — so the accepted candidate participates in selector and lifecycle events.
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("new-cand");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let acceptance_evidence = EvidenceRef::try_new("trace-abc").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "new-cand".to_string(),
                acceptance_evidence,
                tick: 2,
            },
        );

        assert!(
            state.accepted_candidates.contains_key("new-cand"),
            "goal data must be in accepted_candidates"
        );
        assert!(
            state.goals.contains_key("new-cand"),
            "dynamic state must be in state.goals for selector and lifecycle wiring"
        );
        assert!(
            !fixture.goals.iter().any(|g| g.id == "new-cand"),
            "accepted candidate must not be in the static fixture"
        );
    }

    // ── propose_goal_candidates ──────────────────────────────────────────────────

    use super::propose_goal_candidates;

    #[test]
    fn propose_goal_candidates_matched_question_becomes_candidate() {
        let fixture = static_fixture();
        let result = propose_goal_candidates(
            &["Is continuity preserved across sessions?".to_string()],
            &fixture,
        );
        assert_eq!(result.candidates.len(), 1);
        assert!(result.unmatched_questions.is_empty());
        assert!(!result.candidates[0].proposal_evidence().is_empty());
    }

    #[test]
    fn propose_goal_candidates_unmatched_question_goes_to_unmatched_list() {
        let fixture = static_fixture();
        let result = propose_goal_candidates(&["What time is it?".to_string()], &fixture);
        assert!(result.candidates.is_empty());
        assert_eq!(result.unmatched_questions.len(), 1);
    }

    #[test]
    fn propose_goal_candidates_is_deterministic() {
        let fixture = static_fixture();
        let questions = vec![
            "Is continuity preserved across sessions?".to_string(),
            "What time is it?".to_string(),
        ];
        let first = propose_goal_candidates(&questions, &fixture);
        let second = propose_goal_candidates(&questions, &fixture);
        assert_eq!(first.candidates.len(), second.candidates.len());
        for (a, b) in first.candidates.iter().zip(second.candidates.iter()) {
            assert_eq!(a.id(), b.id());
        }
    }

    #[test]
    fn proposed_candidates_have_nonempty_evidence_refs() {
        let fixture = static_fixture();
        let result = propose_goal_candidates(
            &["Research curiosity about unresolved questions.".to_string()],
            &fixture,
        );
        for candidate in &result.candidates {
            assert!(!candidate.proposal_evidence().is_empty());
        }
    }

    #[test]
    fn accepted_candidate_with_no_keywords_does_not_appear_in_selector() {
        // Phase 7: accepted candidates are wired into the selector, but a candidate
        // with empty tension_ids derives no activation_keywords and never matches.
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-selector");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let acceptance_evidence = EvidenceRef::try_new("trace-1").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "cand-selector".to_string(),
                acceptance_evidence,
                tick: 2,
            },
        );

        let result = super::select_goals_with_salience(
            "cand selector",
            &fixture,
            &state,
            ContextBudget::new(4, 200),
        );
        assert!(
            result.selected.iter().all(|s| s.goal.id != "cand-selector"),
            "candidate with no activation keywords must not appear in selector output"
        );
    }

    #[test]
    fn proposed_goal_candidate_deserialization_rejects_empty_evidence() {
        let json = serde_json::json!({
            "id": "test-id",
            "title": "Test",
            "summary": "Summary",
            "tension_ids": [],
            "scope": "session",
            "base_priority": 70,
            "allowed_effects": [],
            "satisfaction_condition_summary": "Resolved.",
            "proposal_evidence": [],
            "source_description": "test",
            "activation_keywords": []
        });
        let result = serde_json::from_value::<ProposedGoalCandidate>(json);
        assert!(
            result.is_err(),
            "deserializing empty proposal_evidence must fail"
        );
    }

    // ── Phase 7: Selector wiring ──────────────────────────────────────────────

    #[test]
    fn propose_goal_candidates_derives_activation_keywords_from_tension_id_parts() {
        let fixture = static_fixture();
        // continuity-preservation → ["continuity", "preservation"]
        let result = propose_goal_candidates(
            &["Is continuity preserved across sessions?".to_string()],
            &fixture,
        );
        assert_eq!(result.candidates.len(), 1);
        let keywords = result.candidates[0].activation_keywords();
        assert!(
            keywords.contains(&"continuity".to_string()),
            "expected 'continuity' in keywords, got: {keywords:?}"
        );
        assert!(
            keywords.contains(&"preservation".to_string()),
            "expected 'preservation' in keywords, got: {keywords:?}"
        );
    }

    #[test]
    fn accepted_candidate_gets_goal_dynamic_state_on_acceptance() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let questions = vec!["Is continuity preserved across sessions?".to_string()];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let candidate = &proposal.candidates[0];
        let candidate_id = candidate.id().to_string();

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: continuity accepted").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence: evidence,
                tick: 2,
            },
        );

        assert!(
            state.goals.contains_key(&candidate_id),
            "accepted candidate must have a GoalDynamicState entry in state.goals"
        );
        let dynamic = state.goals.get(&candidate_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Accepted);
        assert_eq!(dynamic.salience, 0);
    }

    #[test]
    fn accepted_candidate_with_derived_keywords_appears_in_selector() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let questions = vec!["Is continuity preserved across sessions?".to_string()];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let candidate = &proposal.candidates[0];
        let candidate_id = candidate.id().to_string();

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: accepted for selector wiring test").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence: evidence,
                tick: 2,
            },
        );

        // "continuity" matches keywords derived from "continuity-preservation"
        let result = super::select_goals_with_salience(
            "Is continuity still preserved?",
            &fixture,
            &state,
            ContextBudget::new(4, 200),
        );

        assert!(
            result.selected.iter().any(|s| s.goal.id == candidate_id),
            "accepted candidate must appear in selector output when input matches its derived keywords"
        );
    }

    #[test]
    fn accepted_candidate_uses_same_dynamic_state_path_as_fixture_goals() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let questions = vec!["Is continuity preserved across sessions?".to_string()];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let candidate = &proposal.candidates[0];
        let candidate_id = candidate.id().to_string();

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: accepted").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence: evidence,
                tick: 2,
            },
        );
        // Apply an activation event — same reducer branch as fixture goals.
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: candidate_id.clone(),
                tick: 3,
            },
        );

        let dynamic = state.goals.get(&candidate_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Active);
        assert_eq!(dynamic.salience, SALIENCE_ACTIVATION_BONUS);
    }

    // ── Phase 7: Initiative execution ─────────────────────────────────────────

    use super::{InitiativeOutput, execute_initiative};

    #[test]
    fn execute_initiative_reflect_returns_reflection_requested() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let initiative = InitiativeProposal {
            goal_id: goal.id.clone(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::Reflect,
            rationale: "test".to_string(),
            matched_terms: vec!["memory".to_string()],
            scope: goal.scope,
        };
        let output = execute_initiative(&initiative, goal);
        assert!(
            matches!(output, InitiativeOutput::ReflectionRequested { .. }),
            "Reflect effect must produce ReflectionRequested"
        );
    }

    #[test]
    fn execute_initiative_retrieve_context_returns_context_retrieval_requested() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "resurface-open-thread")
            .unwrap();
        let initiative = InitiativeProposal {
            goal_id: goal.id.clone(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::RetrieveContext,
            rationale: "test".to_string(),
            matched_terms: vec!["continuity".to_string(), "thread".to_string()],
            scope: goal.scope,
        };
        let output = execute_initiative(&initiative, goal);
        match output {
            InitiativeOutput::ContextRetrievalRequested { query_terms } => {
                assert_eq!(
                    query_terms,
                    vec!["continuity".to_string(), "thread".to_string()]
                );
            }
            other => panic!("expected ContextRetrievalRequested, got: {other:?}"),
        }
    }

    #[test]
    fn execute_initiative_propose_experiment_returns_experiment_proposed() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "propose-followup-experiment")
            .unwrap();
        let initiative = InitiativeProposal {
            goal_id: goal.id.clone(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::ProposeExperiment,
            rationale: "test".to_string(),
            matched_terms: vec!["experiment".to_string()],
            scope: goal.scope,
        };
        let output = execute_initiative(&initiative, goal);
        assert!(
            matches!(output, InitiativeOutput::ExperimentProposed { .. }),
            "ProposeExperiment effect must produce ExperimentProposed"
        );
    }

    #[test]
    fn execute_initiative_surface_thread_returns_open_thread_surfaced() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "resurface-open-thread")
            .unwrap();
        let initiative = InitiativeProposal {
            goal_id: goal.id.clone(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::SurfaceOpenThread,
            rationale: "test".to_string(),
            matched_terms: vec!["thread".to_string()],
            scope: goal.scope,
        };
        let output = execute_initiative(&initiative, goal);
        assert!(
            matches!(output, InitiativeOutput::OpenThreadSurfaced { .. }),
            "SurfaceOpenThread effect must produce OpenThreadSurfaced"
        );
    }

    #[test]
    fn execute_initiative_is_deterministic() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let initiative = InitiativeProposal {
            goal_id: goal.id.clone(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::Reflect,
            rationale: "test".to_string(),
            matched_terms: vec!["memory".to_string()],
            scope: goal.scope,
        };
        assert_eq!(
            execute_initiative(&initiative, goal),
            execute_initiative(&initiative, goal)
        );
    }

    #[test]
    fn initiative_executed_sets_goal_active_and_records_tick() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let output = InitiativeOutput::ReflectionRequested {
            proposed_question: "What is unclear about voice memory?".to_string(),
        };

        let state = apply(
            state,
            VolitionEvent::InitiativeExecuted {
                goal_id: goal_id.to_string(),
                effect: AllowedEffect::Reflect,
                output,
                rationale: "test rationale".to_string(),
                tick: 3,
            },
        );

        let dynamic = state.goal(goal_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Active);
        assert_eq!(dynamic.last_activated_tick, Some(3));
        assert!(dynamic.last_initiative_output.is_some());
    }

    #[test]
    fn initiative_executed_stores_output_in_dynamic_state() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let expected_output = InitiativeOutput::ReflectionRequested {
            proposed_question: "What is unclear about voice memory?".to_string(),
        };

        let state = apply(
            state,
            VolitionEvent::InitiativeExecuted {
                goal_id: goal_id.to_string(),
                effect: AllowedEffect::Reflect,
                output: expected_output.clone(),
                rationale: "test".to_string(),
                tick: 1,
            },
        );

        assert_eq!(
            state.goal(goal_id).unwrap().last_initiative_output.as_ref(),
            Some(&expected_output)
        );
    }

    #[test]
    fn initiative_executed_unknown_goal_id_is_noop_on_goals() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goals_before = state.goals.clone();
        let output = InitiativeOutput::ReflectionRequested {
            proposed_question: "Unused".to_string(),
        };

        let state_after = apply(
            state,
            VolitionEvent::InitiativeExecuted {
                goal_id: "nonexistent-goal".to_string(),
                effect: AllowedEffect::Reflect,
                output,
                rationale: "test".to_string(),
                tick: 1,
            },
        );

        assert_eq!(state_after.goals, goals_before);
    }

    // ── Phase 8: Mode and arbitrate_with_mode ─────────────────────────────────

    use super::{BiasOutcome, Mode, PROTECTED_TIER_FLOOR, arbitrate_with_mode};

    #[test]
    fn mode_neutral_bias_vector_is_empty() {
        assert!(Mode::Neutral.bias_vector().is_empty());
    }

    #[test]
    fn mode_focused_bias_vector_matches_spec() {
        let vec = Mode::Focused.bias_vector();
        assert_eq!(vec.get("research-curiosity"), Some(&3i8));
        assert_eq!(vec.get("continuity-preservation"), Some(&-1i8));
        assert_eq!(vec.len(), 2);
    }

    #[test]
    fn mode_exploratory_bias_vector_matches_spec() {
        let vec = Mode::Exploratory.bias_vector();
        assert_eq!(vec.get("research-curiosity"), Some(&-2i8));
        assert_eq!(vec.get("continuity-preservation"), Some(&1i8));
        assert_eq!(vec.len(), 2);
    }

    #[test]
    fn arbitrate_with_mode_neutral_matches_arbitrate() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        // Band-only conflict: resurface-open-thread (tier 5) vs clarify-weak-evidence-topic (tier 7)
        let input = "The open thread about voice memory evidence is unresolved.";
        let sel =
            super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));
        assert_eq!(sel.selected.len(), 2, "expected 2 band-only selections");

        let arb = arbitrate(sel.selected.clone(), &fixture).unwrap();
        let mode_arb = arbitrate_with_mode(sel.selected, &fixture, Mode::Neutral).unwrap();

        assert_eq!(arb.winner.goal.id, mode_arb.winner.goal.id);
        assert_eq!(
            arb.winner_effective_tier,
            mode_arb.winner_bias.effective_tier
        );
        assert_eq!(
            arb.winner_effective_tension_id,
            mode_arb.winner_effective_tension_id
        );
        assert_eq!(arb.losers.len(), mode_arb.losers.len());
        for (al, ml) in arb.losers.iter().zip(mode_arb.losers.iter()) {
            assert_eq!(al.selection.goal.id, ml.selection.goal.id);
            assert_eq!(al.effective_tier, ml.bias.effective_tier);
        }
    }

    #[test]
    fn arbitrate_with_mode_exploratory_flips_band_winner() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "The open thread about voice memory evidence is unresolved.";
        let sel =
            super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));

        let neutral = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Neutral).unwrap();
        let exploratory = arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory).unwrap();

        // Under Neutral: continuity (tier 5) beats curiosity (tier 7)
        assert_eq!(neutral.winner.goal.id, "resurface-open-thread");
        // Under Exploratory: curiosity biased to 5, continuity biased to 6 → curiosity wins
        assert_eq!(exploratory.winner.goal.id, "clarify-weak-evidence-topic");
        assert_ne!(neutral.winner.goal.id, exploratory.winner.goal.id);
    }

    #[test]
    fn arbitrate_with_mode_floor_goal_wins_under_biasing_mode() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        // Floor input: avoid-overstating-impl-status (tier 1) + two band goals
        let input =
            "Is the voice memory work complete, or is the evidence thread still unresolved?";
        let sel =
            super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));
        assert!(
            sel.selected
                .iter()
                .any(|s| s.goal.id == "avoid-overstating-impl-status"),
            "floor goal must be selected"
        );

        let neutral = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Neutral).unwrap();
        let exploratory = arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory).unwrap();

        // Floor goal wins under both modes
        assert_eq!(neutral.winner.goal.id, "avoid-overstating-impl-status");
        assert_eq!(exploratory.winner.goal.id, "avoid-overstating-impl-status");
        assert!(
            exploratory.winner_bias.protected,
            "floor goal must be marked protected"
        );
    }

    #[test]
    fn arbitrate_with_mode_focused_keeps_continuity_winner() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "The open thread about voice memory evidence is unresolved.";
        let sel =
            super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));

        let focused = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Focused).unwrap();

        // Under Focused: continuity biased to 4, curiosity biased to 10 → continuity wins
        assert_eq!(focused.winner.goal.id, "resurface-open-thread");

        // Curiosity (clarify-weak-evidence-topic) should be demoted
        let curiosity_loser = focused
            .losers
            .iter()
            .find(|l| l.selection.goal.id == "clarify-weak-evidence-topic")
            .expect("curiosity goal must be a loser under Focused");
        assert!(
            curiosity_loser.bias.bias_applied > 0,
            "curiosity bias_applied must be positive (demotion) under Focused"
        );
        assert!(
            curiosity_loser.bias.biased_tier > curiosity_loser.bias.effective_tier,
            "curiosity biased_tier must exceed effective_tier under Focused"
        );
    }

    #[test]
    fn mode_changed_event_updates_state_mode() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        assert_eq!(state.mode, Mode::Neutral, "initial mode must be Neutral");

        let state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 1,
            },
        );
        assert_eq!(state.mode, Mode::Exploratory);

        let state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Focused,
                tick: 2,
            },
        );
        assert_eq!(state.mode, Mode::Focused);

        let state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Neutral,
                tick: 3,
            },
        );
        assert_eq!(state.mode, Mode::Neutral);
    }

    #[test]
    fn mode_changed_replay_reproduces_mode() {
        let fixture = static_fixture();
        let apply_seq = || {
            let s = VolitionState::from_fixture(&fixture);
            let s = apply(
                s,
                VolitionEvent::ModeChanged {
                    mode: Mode::Exploratory,
                    tick: 1,
                },
            );
            apply(
                s,
                VolitionEvent::ModeChanged {
                    mode: Mode::Focused,
                    tick: 2,
                },
            )
        };
        assert_eq!(apply_seq().mode, apply_seq().mode);
    }

    #[test]
    fn band_goal_biased_tier_never_enters_floor() {
        // Verify the clamp arithmetic: a large negative bias on a band goal cannot produce
        // a biased_tier below PROTECTED_TIER_FLOOR + 1.
        let effective_tier: u8 = 5;
        let bias_applied: i8 = i8::MIN; // -128, extreme promotion attempt
        let raw = effective_tier as i16 + bias_applied as i16; // 5 - 128 = -123
        let biased_tier = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8;
        assert_eq!(biased_tier, PROTECTED_TIER_FLOOR + 1);

        // Confirm the BiasOutcome constructed from this arithmetic is correct.
        let outcome = BiasOutcome {
            effective_tier,
            bias_applied,
            biased_tier,
            protected: false,
        };
        assert_eq!(outcome.biased_tier, PROTECTED_TIER_FLOOR + 1);
        assert!(!outcome.protected);
    }

    #[test]
    fn bias_arithmetic_u8_max_stays_at_max_under_positive_demotion() {
        // A goal with no fixture tension gets effective_tier = u8::MAX.
        // A large positive demotion must not wrap or panic; it should stay at u8::MAX.
        let fixture = VolitionFixture {
            tensions: vec![],
            goals: vec![],
        };
        let sel = make_goal_for_arbitration("no-tension-goal", vec![], 80);
        let result = arbitrate_with_mode(vec![sel], &fixture, Mode::Focused).unwrap();
        assert_eq!(result.winner_bias.effective_tier, u8::MAX);
        assert_eq!(result.winner_bias.biased_tier, u8::MAX);
    }

    #[test]
    fn arbitrate_with_mode_is_deterministic() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "The open thread about voice memory evidence is unresolved.";
        let budget = ContextBudget::new(4, 100);

        let run = || {
            let sel = super::select_goals_with_salience(input, &fixture, &state, budget);
            arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory)
        };

        let r1 = run().unwrap();
        let r2 = run().unwrap();
        assert_eq!(r1.winner.goal.id, r2.winner.goal.id);
        assert_eq!(r1.winner_bias.biased_tier, r2.winner_bias.biased_tier);
    }

    #[test]
    fn mode_field_serde_default_is_neutral() {
        // Existing state serialized without `mode` field must deserialize as Neutral.
        let json = serde_json::json!({
            "tick": 0,
            "goals": {},
            "pending_candidates": [],
            "accepted_candidates": {}
        });
        let state: VolitionState = serde_json::from_value(json).unwrap();
        assert_eq!(state.mode, Mode::Neutral);
    }
}
