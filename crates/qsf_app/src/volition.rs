use std::collections::BTreeMap;
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

/// A persistent pressure that names what the system cares about. Tensions back goals and
/// determine arbitration precedence when multiple goals compete.
///
/// ## Arbitration tier
///
/// `arbitration_tier` places this tension in the conflict-resolution hierarchy. A goal's
/// effective tier is the minimum `arbitration_tier` among its parent tensions (defaulting
/// to `u8::MAX` if it has no parent tensions in the fixture). Lower tier wins.
///
/// Covered tiers in the current fixture:
/// - **1** — Safety and project boundaries (`boundary-preservation`)
/// - **4** — Coherence and self-correction (`coherence-maintenance`)
/// - **5** — Continuity preservation (`continuity-preservation`)
/// - **7** — Research curiosity (`research-curiosity`)
///
/// Extension points (not yet covered by any fixture tension):
/// - **2** — Explicit user intent
/// - **3** — Current task completion
/// - **6** — Active experiment mode
/// - **8** — Optional exploration
///
/// Future tensions must be assigned the correct tier when added. A goal with effective
/// tier `u8::MAX` (no parent tensions in the fixture) is a signal that a tension
/// assignment is missing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Tension {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub priority_bias: TensionPriority,
    /// Arbitration precedence tier; lower tier wins conflict resolution. See type doc.
    pub arbitration_tier: u8,
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
    Active,
    Blocked,
    Satisfied,
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

/// A goal selection that lost cross-goal arbitration. Records the full selection plus
/// the structured tension provenance that determined its effective tier.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArbitrationLoser {
    pub selection: GoalSelection,
    /// The effective arbitration tier for this goal (minimum tier among parent tensions).
    pub effective_tier: u8,
    /// The tension responsible for this goal's effective tier.
    pub effective_tension_id: String,
    /// Human-readable name of the effective tension.
    pub effective_tension_title: String,
    /// Rendered convenience reason, e.g. "tier 7 lost to winner at tier 1 (boundary-preservation)".
    /// Tests must assert the structured fields above, not this string.
    pub reason: String,
}

/// The result of deterministic cross-goal arbitration. The winner is the goal with the
/// lowest effective tier (minimum `arbitration_tier` among its parent tensions); ties are
/// broken by higher `base_priority`, then lower `goal_id` lexicographically.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArbitrationResult {
    pub winner: GoalSelection,
    /// Effective tier that placed the winner.
    pub winner_effective_tier: u8,
    /// Tension responsible for the winner's effective tier.
    pub winner_effective_tension_id: String,
    /// Human-readable name of the winner's effective tension.
    pub winner_effective_tension_title: String,
    /// Losing goals sorted: effective tier ascending, base_priority descending, goal_id ascending.
    pub losers: Vec<ArbitrationLoser>,
}

/// Resolve cross-goal conflict by tension tier. Returns `None` for empty input. For a
/// single selection, the sole goal is the winner with an empty losers list. Pure and
/// stateless — reads `fixture` only to resolve tensions for each goal.
pub fn arbitrate(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
) -> Option<ArbitrationResult> {
    if selections.is_empty() {
        return None;
    }

    let mut with_tiers: Vec<(GoalSelection, u8, String, String)> = selections
        .into_iter()
        .map(|selection| {
            let (tier, tension_id, tension_title) =
                effective_tension_for_goal(&selection.goal, fixture);
            (selection, tier, tension_id, tension_title)
        })
        .collect();

    // Sort: effective tier ascending, base_priority descending, goal_id ascending.
    with_tiers.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then(b.0.goal.base_priority.cmp(&a.0.goal.base_priority))
            .then(a.0.goal.id.cmp(&b.0.goal.id))
    });

    let (winner_sel, winner_tier, winner_tension_id, winner_tension_title) = with_tiers.remove(0);

    let losers = with_tiers
        .into_iter()
        .map(|(sel, tier, tension_id, tension_title)| {
            let reason = format!(
                "tier {} lost to winner at tier {} ({})",
                tier, winner_tier, winner_tension_id
            );
            ArbitrationLoser {
                selection: sel,
                effective_tier: tier,
                effective_tension_id: tension_id,
                effective_tension_title: tension_title,
                reason,
            }
        })
        .collect();

    Some(ArbitrationResult {
        winner: winner_sel,
        winner_effective_tier: winner_tier,
        winner_effective_tension_id: winner_tension_id,
        winner_effective_tension_title: winner_tension_title,
        losers,
    })
}

/// Returns the `(effective_tier, tension_id, tension_title)` for a goal. The effective
/// tier is the minimum `arbitration_tier` among the goal's parent tensions in the fixture.
/// When multiple tensions share the minimum tier, the lexicographically smallest
/// `tension_id` is chosen as the effective tension. Returns `(u8::MAX, "", "")` when the
/// goal has no parent tensions in the fixture.
fn effective_tension_for_goal(goal: &Goal, fixture: &VolitionFixture) -> (u8, String, String) {
    let parent_tensions: Vec<&Tension> = goal
        .tension_ids
        .iter()
        .filter_map(|tension_id| {
            fixture
                .tensions
                .iter()
                .find(|tension| tension.id == *tension_id)
        })
        .collect();

    if parent_tensions.is_empty() {
        return (u8::MAX, String::new(), String::new());
    }

    let min_tier = parent_tensions
        .iter()
        .map(|tension| tension.arbitration_tier)
        .min()
        .unwrap();

    let effective = parent_tensions
        .iter()
        .filter(|tension| tension.arbitration_tier == min_tier)
        .min_by_key(|tension| &tension.id)
        .unwrap();

    (min_tier, effective.id.clone(), effective.title.clone())
}

/// A non-empty, non-whitespace reference to an observable artifact or trace that
/// justifies a progress or satisfaction event. Cannot be constructed from empty or
/// whitespace-only input — use `EvidenceRef::try_new` or `TryFrom<String>`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceRef(String);

impl EvidenceRef {
    pub fn try_new(value: impl Into<String>) -> Result<Self, EvidenceRefError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(EvidenceRefError::Empty);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EvidenceRef {
    type Error = EvidenceRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl fmt::Display for EvidenceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceRefError {
    Empty,
}

impl fmt::Display for EvidenceRefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("evidence ref must not be empty or whitespace-only")
    }
}

/// Dynamic, per-goal state tracked within a run. Separate from the read-only fixture.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalDynamicState {
    pub status: GoalStatus,
    /// Integer salience points; rises on activation/progress, decays per tick linearly.
    pub salience: i32,
    pub reinforcement_count: u32,
    pub progress_evidence_refs: Vec<EvidenceRef>,
    pub last_activated_tick: Option<u64>,
    pub last_satisfied_tick: Option<u64>,
    /// Tick at which cooldown ends and the goal returns to Accepted.
    pub cooldown_until_tick: Option<u64>,
}

impl GoalDynamicState {
    fn initial() -> Self {
        Self {
            status: GoalStatus::Accepted,
            salience: 0,
            reinforcement_count: 0,
            progress_evidence_refs: Vec::new(),
            last_activated_tick: None,
            last_satisfied_tick: None,
            cooldown_until_tick: None,
        }
    }
}

/// Durable-within-a-run volition state: a logical tick and per-goal dynamic state for
/// all Accepted goals seeded from the fixture.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionState {
    pub tick: u64,
    /// Keyed by goal id.
    pub goals: BTreeMap<String, GoalDynamicState>,
}

impl VolitionState {
    /// Seed initial state from the fixture's Accepted goals.
    pub fn from_fixture(fixture: &VolitionFixture) -> Self {
        let goals = fixture
            .goals
            .iter()
            .filter(|goal| goal.status == GoalStatus::Accepted)
            .map(|goal| (goal.id.clone(), GoalDynamicState::initial()))
            .collect();
        Self { tick: 0, goals }
    }

    pub fn goal(&self, goal_id: &str) -> Option<&GoalDynamicState> {
        self.goals.get(goal_id)
    }
}

/// Salience points added when a goal is activated (first keyword match in a turn).
pub const SALIENCE_ACTIVATION_BONUS: i32 = 10;
/// Salience points added when progress evidence is recorded.
pub const SALIENCE_PROGRESS_BONUS: i32 = 5;
/// Salience points lost per tick from GoalDecayed.
pub const SALIENCE_DECAY_PER_TICK: i32 = 2;
/// Ticks of cooldown after a goal is satisfied.
pub const COOLDOWN_SPAN_TICKS: u64 = 3;
/// Ticks of inactivity after which an unproductive goal is retired.
pub const RETIREMENT_INACTIVITY_TICKS: u64 = 10;

/// One event per explicit lifecycle transition. The tick is the monotonic counter at the
/// time the event is produced; the reducer uses it to set timestamp fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum VolitionEvent {
    GoalActivated {
        goal_id: String,
        tick: u64,
    },
    GoalProgressObserved {
        goal_id: String,
        evidence: EvidenceRef,
        tick: u64,
    },
    GoalSatisfied {
        goal_id: String,
        evidence: EvidenceRef,
        tick: u64,
    },
    GoalBlocked {
        goal_id: String,
        tick: u64,
    },
    /// Salience-only decay; never changes status.
    GoalDecayed {
        goal_id: String,
        tick: u64,
    },
    /// Transitions a Cooldown goal back to Accepted.
    GoalCooldownElapsed {
        goal_id: String,
        tick: u64,
    },
    GoalRetired {
        goal_id: String,
        tick: u64,
    },
    /// Advances the logical tick without modifying any goal lifecycle state.
    /// Applied unconditionally each turn to guarantee state.tick is monotonically
    /// increasing even when no lifecycle events are emitted.
    TickAdvanced {
        tick: u64,
    },
}

/// Pure reducer: applies one event to state and returns the next state.
/// The only place lifecycle status changes; selectors never mutate lifecycle.
pub fn apply(mut state: VolitionState, event: VolitionEvent) -> VolitionState {
    state.tick = state.tick.max(event_tick(&event));
    match event {
        VolitionEvent::GoalActivated { goal_id, tick } => {
            let dynamic = state
                .goals
                .entry(goal_id)
                .or_insert_with(GoalDynamicState::initial);
            dynamic.status = GoalStatus::Active;
            dynamic.salience = (dynamic.salience + SALIENCE_ACTIVATION_BONUS).max(0);
            dynamic.last_activated_tick = Some(tick);
        }
        VolitionEvent::GoalProgressObserved {
            goal_id,
            evidence,
            tick: _,
        } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.reinforcement_count += 1;
                dynamic.salience = (dynamic.salience + SALIENCE_PROGRESS_BONUS).max(0);
                dynamic.progress_evidence_refs.push(evidence);
            }
        }
        VolitionEvent::GoalSatisfied {
            goal_id,
            evidence,
            tick,
        } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Cooldown;
                dynamic.salience = 0;
                dynamic.last_satisfied_tick = Some(tick);
                dynamic.cooldown_until_tick = Some(tick + COOLDOWN_SPAN_TICKS);
                dynamic.progress_evidence_refs.push(evidence);
            }
        }
        VolitionEvent::GoalBlocked { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Blocked;
            }
        }
        VolitionEvent::GoalDecayed { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.salience = (dynamic.salience - SALIENCE_DECAY_PER_TICK).max(0);
            }
        }
        VolitionEvent::GoalCooldownElapsed { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Accepted;
                dynamic.cooldown_until_tick = None;
            }
        }
        VolitionEvent::GoalRetired { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Retired;
            }
        }
        VolitionEvent::TickAdvanced { .. } => {}
    }
    state
}

fn event_tick(event: &VolitionEvent) -> u64 {
    match event {
        VolitionEvent::GoalActivated { tick, .. }
        | VolitionEvent::GoalProgressObserved { tick, .. }
        | VolitionEvent::GoalSatisfied { tick, .. }
        | VolitionEvent::GoalBlocked { tick, .. }
        | VolitionEvent::GoalDecayed { tick, .. }
        | VolitionEvent::GoalCooldownElapsed { tick, .. }
        | VolitionEvent::GoalRetired { tick, .. }
        | VolitionEvent::TickAdvanced { tick } => *tick,
    }
}

/// Given the current state and the next tick, returns any tick-driven events that should
/// be applied: decay for all active/accepted goals, cooldown-elapsed for goals whose
/// cooldown has ended, retirement for goals that have been inactive too long.
pub fn tick_events(state: &VolitionState, new_tick: u64) -> Vec<VolitionEvent> {
    let mut events = Vec::new();
    for (goal_id, dynamic) in &state.goals {
        match dynamic.status {
            GoalStatus::Cooldown => {
                if let Some(cooldown_until) = dynamic.cooldown_until_tick {
                    if new_tick >= cooldown_until {
                        events.push(VolitionEvent::GoalCooldownElapsed {
                            goal_id: goal_id.clone(),
                            tick: new_tick,
                        });
                    }
                }
            }
            GoalStatus::Active | GoalStatus::Accepted => {
                if dynamic.salience > 0 {
                    events.push(VolitionEvent::GoalDecayed {
                        goal_id: goal_id.clone(),
                        tick: new_tick,
                    });
                }
                let last_active = dynamic.last_activated_tick.unwrap_or(0);
                if new_tick.saturating_sub(last_active) >= RETIREMENT_INACTIVITY_TICKS
                    && dynamic.reinforcement_count == 0
                    && dynamic.salience == 0
                {
                    events.push(VolitionEvent::GoalRetired {
                        goal_id: goal_id.clone(),
                        tick: new_tick,
                    });
                }
            }
            GoalStatus::Blocked => {
                if dynamic.salience > 0 {
                    events.push(VolitionEvent::GoalDecayed {
                        goal_id: goal_id.clone(),
                        tick: new_tick,
                    });
                }
            }
            GoalStatus::Proposed | GoalStatus::Satisfied | GoalStatus::Retired => {}
        }
    }
    events
}

pub fn static_fixture() -> VolitionFixture {
    VolitionFixture {
        tensions: vec![
            Tension {
                id: "research-curiosity".to_string(),
                title: "Research curiosity".to_string(),
                summary: "Keep unresolved technical questions visible long enough to compare candidate designs.".to_string(),
                priority_bias: TensionPriority::Medium,
                arbitration_tier: 7,
            },
            Tension {
                id: "coherence-maintenance".to_string(),
                title: "Coherence maintenance".to_string(),
                summary: "Avoid overstating implementation status or blending speculative ideas into current fact.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 4,
            },
            Tension {
                id: "continuity-preservation".to_string(),
                title: "Continuity preservation".to_string(),
                summary: "Keep open threads and unresolved context available across turns.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 5,
            },
            Tension {
                id: "boundary-preservation".to_string(),
                title: "Boundary preservation".to_string(),
                summary: "Protect the distinction between current code, future experiments, and out-of-scope ideas.".to_string(),
                priority_bias: TensionPriority::Highest,
                arbitration_tier: 1,
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
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Satisfied => "satisfied",
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
        let events = tick_events(&state, 2);

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

        let events = tick_events(&state, RETIREMENT_INACTIVITY_TICKS);

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

        let events = tick_events(&state, 2 + COOLDOWN_SPAN_TICKS);

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
    use crate::context::{ContextFragment, ContextSourceKind};

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
            context_fragment: ContextFragment {
                fragment_id: goal.id.clone(),
                source_kind: ContextSourceKind::RuntimeState,
                summary: goal.summary.clone(),
                tags: vec![],
                score: goal.base_priority as f64,
                estimated_tokens: goal.estimated_tokens,
                source_reference: goal.source_reference.clone(),
                selection_reason: "test".to_string(),
            },
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
}
