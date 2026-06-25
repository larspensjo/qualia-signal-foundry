use std::fmt;

use serde::{Deserialize, Serialize};

use crate::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextSourceKind, assemble_context,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionFixture {
    pub tensions: Vec<Tension>,
    pub goals: Vec<Goal>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Tension {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub priority_bias: TensionPriority,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TensionPriority {
    Lowest,
    Low,
    Medium,
    High,
    Highest,
}

impl TensionPriority {
    fn score_bonus(self) -> f64 {
        match self {
            Self::Lowest => 0.0,
            Self::Low => 5.0,
            Self::Medium => 10.0,
            Self::High => 15.0,
            Self::Highest => 20.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub tension_ids: Vec<String>,
    pub status: GoalStatus,
    pub scope: GoalScope,
    pub base_priority: u8,
    pub activation_keywords: Vec<String>,
    pub allowed_effects: Vec<AllowedEffect>,
    pub satisfaction_condition_summary: String,
    pub evidence_refs: Vec<String>,
    pub estimated_tokens: usize,
    pub source_reference: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Proposed,
    Accepted,
    Cooldown,
    Retired,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalScope {
    Input,
    Session,
    Project,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowedEffect {
    Reflect,
    RetrieveContext,
    ProposeExperiment,
    SurfaceOpenThread,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InitiativeProposal {
    pub goal_id: String,
    pub goal_title: String,
    pub effect: AllowedEffect,
    pub rationale: String,
    pub matched_terms: Vec<String>,
    pub scope: GoalScope,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalSelection {
    pub goal: Goal,
    pub context_fragment: ContextFragment,
    pub relevance_score: f64,
    pub matched_terms: Vec<String>,
    pub initiative: InitiativeProposal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OmittedGoal {
    pub goal: Goal,
    pub relevance_score: f64,
    pub matched_terms: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalSelectionResult {
    pub input: String,
    pub input_terms: Vec<String>,
    pub budget: ContextBudget,
    pub selected: Vec<GoalSelection>,
    pub omitted: Vec<OmittedGoal>,
    pub assembly: ContextAssembly,
}

/// Explicit reminder that tension priority bias is recorded as provenance only and is
/// not treated as a proven selection mechanism in the trace-backed-initiative slice.
pub const TENSION_PRIORITY_NOTE: &str = "Tensions are recorded as goal provenance only; \
their priority bias did not determine selection and is not treated as proven architecture.";

/// Inspectable provenance for a tension that contributed to a selected goal. Recorded
/// for legibility, not as evidence that tension priority drove selection.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TensionProvenance {
    pub tension_id: String,
    pub title: String,
    pub priority_bias: TensionPriority,
}

/// A detected discrepancy between the input and a goal's concern. Cites the input
/// evidence that matched and the goal's own satisfaction/concern summary so the delta
/// stays more informative than a bare keyword match.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DetectedDelta {
    pub matched_evidence: Vec<String>,
    pub goal_concern_summary: String,
}

/// Whether an input produced a goal-relevant delta or an explicit, recorded no-delta
/// reason. Baseline inputs must carry `NoDelta` so the absence of an initiative is
/// legible rather than implicit.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DeltaAssessment {
    Delta(DetectedDelta),
    NoDelta { reason: String },
}

/// A candidate initiative that lost the local, single-goal choice, with a deterministic
/// precedence-based rejection reason. This is trace scaffolding, not cross-goal
/// arbitration.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LosingCandidate {
    pub proposal: InitiativeProposal,
    pub reason: String,
}

/// The local choice between candidate initiatives derived from a single selected goal:
/// the proposed (winning) bounded effect plus the losing candidates and why they lost.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InitiativeChoice {
    pub proposed: InitiativeProposal,
    pub losing: Vec<LosingCandidate>,
}

/// A pre-initiative trace recorded before any behavior could change. It connects an
/// active goal to its tension provenance, the detected delta (or explicit no-delta
/// reason), the candidate initiatives, and the proposed bounded effect — while
/// executing nothing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PreInitiativeTrace {
    pub input: String,
    pub goal_id: Option<String>,
    pub goal_title: Option<String>,
    pub goal_summary: Option<String>,
    pub tensions: Vec<TensionProvenance>,
    pub tension_priority_note: String,
    pub delta: DeltaAssessment,
    pub choice: Option<InitiativeChoice>,
    pub allowed_rationale: Option<String>,
    pub executed: bool,
}

pub fn static_fixture() -> VolitionFixture {
    VolitionFixture {
        tensions: vec![
            Tension {
                id: "research-curiosity".to_string(),
                title: "Research curiosity".to_string(),
                summary: "Keep unresolved technical questions visible long enough to compare candidate designs.".to_string(),
                priority_bias: TensionPriority::Medium,
            },
            Tension {
                id: "coherence-maintenance".to_string(),
                title: "Coherence maintenance".to_string(),
                summary: "Avoid overstating implementation status or blending speculative ideas into current fact.".to_string(),
                priority_bias: TensionPriority::High,
            },
            Tension {
                id: "continuity-preservation".to_string(),
                title: "Continuity preservation".to_string(),
                summary: "Keep open threads and unresolved context available across turns.".to_string(),
                priority_bias: TensionPriority::High,
            },
            Tension {
                id: "boundary-preservation".to_string(),
                title: "Boundary preservation".to_string(),
                summary: "Protect the distinction between current code, future experiments, and out-of-scope ideas.".to_string(),
                priority_bias: TensionPriority::Highest,
            },
        ],
        goals: vec![
            Goal {
                id: "clarify-weak-evidence-topic".to_string(),
                title: "Clarify weak evidence topic".to_string(),
                summary: "Surface a research question when the input points at uncertain or under-explained material.".to_string(),
                tension_ids: vec!["research-curiosity".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 85,
                activation_keywords: vec![
                    "voice".to_string(),
                    "memory".to_string(),
                    "evidence".to_string(),
                    "unclear".to_string(),
                    "unsettled".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect, AllowedEffect::ProposeExperiment],
                satisfaction_condition_summary: "The uncertain topic has been named clearly enough to compare options or ask a narrower question.".to_string(),
                evidence_refs: vec![
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                    "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
                ],
                estimated_tokens: 20,
                source_reference: "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
            },
            Goal {
                id: "avoid-overstating-impl-status".to_string(),
                title: "Avoid overstating implementation status".to_string(),
                summary: "Keep status claims grounded when the input asks whether the volition work is actually done.".to_string(),
                tension_ids: vec![
                    "coherence-maintenance".to_string(),
                    "boundary-preservation".to_string(),
                ],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 95,
                activation_keywords: vec![
                    "implemented".to_string(),
                    "status".to_string(),
                    "complete".to_string(),
                    "done".to_string(),
                    "ready".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "The response avoids claiming completion that the current repository state does not support.".to_string(),
                evidence_refs: vec![
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                    "docs/DecisionLog.md".to_string(),
                ],
                estimated_tokens: 18,
                source_reference: "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
            },
            Goal {
                id: "resurface-open-thread".to_string(),
                title: "Resurface open thread".to_string(),
                summary: "Bring an unresolved continuity issue back into view when the input mentions continuity or an open thread.".to_string(),
                tension_ids: vec!["continuity-preservation".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 98,
                activation_keywords: vec![
                    "continuity".to_string(),
                    "thread".to_string(),
                    "revisit".to_string(),
                    "open".to_string(),
                    "unresolved".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::RetrieveContext, AllowedEffect::SurfaceOpenThread],
                satisfaction_condition_summary: "The unresolved thread is named well enough that the next turn can carry it forward.".to_string(),
                evidence_refs: vec![
                    "docs/Architecture/Architecture.ContextManagement.md".to_string(),
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                ],
                estimated_tokens: 24,
                source_reference: "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
            },
            Goal {
                id: "propose-followup-experiment".to_string(),
                title: "Propose follow-up experiment".to_string(),
                summary: "Suggest a bounded follow-up experiment when the conversation is already in research mode.".to_string(),
                tension_ids: vec!["research-curiosity".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 90,
                activation_keywords: vec![
                    "experiment".to_string(),
                    "compare".to_string(),
                    "perturbation".to_string(),
                    "fixture".to_string(),
                    "prototype".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::ProposeExperiment],
                satisfaction_condition_summary: "A concrete follow-up experiment has been described in a way that can be run later.".to_string(),
                evidence_refs: vec![
                    "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                ],
                estimated_tokens: 22,
                source_reference: "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
            },
        ],
    }
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
            context_fragment: selection.fragment.clone(),
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

fn normalize_terms(input: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();

    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            if !terms.iter().any(|term| term == &current) {
                terms.push(current.clone());
            }
            current.clear();
        }
    }

    if !current.is_empty() && !terms.iter().any(|term| term == &current) {
        terms.push(current);
    }

    terms
}

#[derive(Clone)]
struct GoalEvaluation {
    goal: Goal,
    matched_terms: Vec<String>,
    relevance_score: f64,
    fragment: ContextFragment,
}

impl fmt::Display for GoalStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Cooldown => "cooldown",
            Self::Retired => "retired",
        })
    }
}

impl fmt::Display for GoalScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Input => "input",
            Self::Session => "session",
            Self::Project => "project",
        })
    }
}

impl fmt::Display for AllowedEffect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Reflect => "reflect",
            Self::RetrieveContext => "retrieve-context",
            Self::ProposeExperiment => "propose-experiment",
            Self::SurfaceOpenThread => "surface-open-thread",
        })
    }
}

impl fmt::Display for TensionPriority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Lowest => "lowest",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Highest => "highest",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DeltaAssessment, GoalSelectionResult, build_pre_initiative_traces, select_goals,
        static_fixture,
    };
    use crate::context::ContextBudget;

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
}
