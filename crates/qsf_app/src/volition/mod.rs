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
    let synthetic_state = VolitionState::from_fixture(fixture);
    let ranked = select_goals_ranked(input, &synthetic_state, fixture);

    let fragments: Vec<ContextFragment> = ranked
        .selected
        .iter()
        .map(|s| build_fragment(&s.goal, s.relevance_score, &s.matched_terms()))
        .collect();
    let assembly = assemble_context(fragments, budget);

    let mut selected = Vec::new();
    for sel in &assembly.selected {
        let s = ranked
            .selected
            .iter()
            .find(|s| s.goal.id == sel.fragment.fragment_id)
            .expect("selected fragment must map back to a ranked goal");
        selected.push(s.clone());
    }

    let mut omitted = ranked.omitted;
    for omission in &assembly.omitted {
        if let Some(s) = ranked
            .selected
            .iter()
            .find(|s| s.goal.id == omission.fragment.fragment_id)
        {
            omitted.push(OmittedGoal {
                goal: s.goal.clone(),
                relevance_score: s.relevance_score,
                matched_terms: s.matched_terms(),
                reason: omission.reason.clone(),
            });
        }
    }

    GoalSelectionResult {
        input: input.to_string(),
        input_terms: ranked.input_terms,
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
    let ranked = select_goals_ranked(input, state, fixture);

    let fragments: Vec<ContextFragment> = ranked
        .selected
        .iter()
        .map(|s| build_fragment(&s.goal, s.relevance_score, &s.matched_terms()))
        .collect();
    let assembly = assemble_context(fragments, budget);

    let mut selected = Vec::new();
    for sel in &assembly.selected {
        let s = ranked
            .selected
            .iter()
            .find(|s| s.goal.id == sel.fragment.fragment_id)
            .expect("selected fragment must map back to a ranked goal");
        selected.push(s.clone());
    }

    let mut omitted = ranked.omitted;
    for omission in &assembly.omitted {
        if let Some(s) = ranked
            .selected
            .iter()
            .find(|s| s.goal.id == omission.fragment.fragment_id)
        {
            omitted.push(OmittedGoal {
                goal: s.goal.clone(),
                relevance_score: s.relevance_score,
                matched_terms: s.matched_terms(),
                reason: omission.reason.clone(),
            });
        }
    }

    SalienceGoalSelectionResult {
        input: input.to_string(),
        input_terms: ranked.input_terms,
        budget,
        selected,
        omitted,
        suppressed_cooldown: ranked.suppressed_cooldown,
        visible_blocked: ranked.visible_blocked,
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
    let choice = initiative_choice(goal, &selection.matched_keywords);
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
            matched_evidence: selection.matched_terms(),
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

fn initiative_choice(goal: &Goal, matched: &[ActivationKeyword]) -> Option<InitiativeChoice> {
    let (chosen_effect, losing_effects) = goal.allowed_effects.split_first()?;
    let proposed = initiative_for_effect(goal, *chosen_effect, matched);

    let losing = losing_effects
        .iter()
        .map(|effect| LosingCandidate {
            proposal: initiative_for_effect(goal, *effect, matched),
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

fn build_fragment(goal: &Goal, relevance_score: f64, matched_terms: &[String]) -> ContextFragment {
    let mut tags: Vec<String> = goal
        .activation_keywords
        .iter()
        .map(|keyword| keyword.term.clone())
        .collect();
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

#[cfg(test)]
mod tests;
