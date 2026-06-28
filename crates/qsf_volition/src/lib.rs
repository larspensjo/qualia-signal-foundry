use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

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
    pub fn score_bonus(self) -> f64 {
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

/// The structural output of a bounded internal initiative. Pure and serializable — one variant
/// per `AllowedEffect`. Records what the runtime *would* do; no external write-capable action.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum InitiativeOutput {
    ReflectionRequested {
        proposed_question: String,
    },
    ContextRetrievalRequested {
        query_terms: Vec<String>,
    },
    ExperimentProposed {
        hypothesis: String,
        scope: GoalScope,
    },
    OpenThreadSurfaced {
        thread_summary: String,
    },
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

/// A selected goal in volition-domain terms: relevance score, matched terms, and proposed
/// initiative. Used as input to arbitration and as an element of selection results.
///
/// Context-neutral by design: the assembled `ContextFragment` for a selection lives in the
/// caller's result shape (see `qsf_app`'s selection results, which carry the full
/// `ContextAssembly`). Keeping it out here makes arbitration a pure volition-domain
/// operation and lets `qsf_volition` stay free of any context dependency.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalSelection {
    pub goal: Goal,
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

/// Tiers 1..=PROTECTED_TIER_FLOOR are immune to mode bias.
pub const PROTECTED_TIER_FLOOR: u8 = 3;

/// An inspectable arbitration bias. Its meaning is its declared `bias_vector()`; the
/// label is only a handle. Default = Neutral (zero bias).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Neutral,
    Focused,
    Exploratory,
}

impl Mode {
    /// Declared bias over tension ids. Negative promotes (lower tier), positive demotes.
    /// Source of truth for the bias; empty for Neutral.
    pub fn bias_vector(self) -> BTreeMap<String, i8> {
        match self {
            Self::Neutral => BTreeMap::new(),
            Self::Focused => {
                let mut map = BTreeMap::new();
                map.insert("research-curiosity".to_string(), 3i8);
                map.insert("continuity-preservation".to_string(), -1i8);
                map
            }
            Self::Exploratory => {
                let mut map = BTreeMap::new();
                map.insert("research-curiosity".to_string(), -2i8);
                map.insert("continuity-preservation".to_string(), 1i8);
                map
            }
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Neutral => "Neutral",
            Self::Focused => "Focused",
            Self::Exploratory => "Exploratory",
        })
    }
}

/// Per-goal record of how mode bias affected this goal's arbitration tier.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BiasOutcome {
    /// Pre-bias effective tier (minimum arbitration_tier among parent tensions).
    pub effective_tier: u8,
    /// Applied bias delta (0 for protected goals, pre-clamp signed delta for band goals).
    pub bias_applied: i8,
    /// Post-bias tier; band goals clamped to >= PROTECTED_TIER_FLOOR + 1.
    pub biased_tier: u8,
    /// True when effective_tier <= PROTECTED_TIER_FLOOR; such goals receive zero bias.
    pub protected: bool,
}

/// A goal selection that lost mode-aware cross-goal arbitration.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModeArbitrationLoser {
    pub selection: GoalSelection,
    pub effective_tension_id: String,
    pub effective_tension_title: String,
    pub bias: BiasOutcome,
    /// Rendered convenience string. Tests must assert structured fields, not this string.
    pub reason: String,
}

/// The result of mode-aware cross-goal arbitration. Sort key: biased_tier asc,
/// base_priority desc, goal_id asc. Floor goals always sort ahead of band goals.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModeArbitrationResult {
    pub mode: Mode,
    pub winner: GoalSelection,
    pub winner_effective_tension_id: String,
    pub winner_effective_tension_title: String,
    pub winner_bias: BiasOutcome,
    pub losers: Vec<ModeArbitrationLoser>,
}

/// Resolve cross-goal conflict by tension tier. Returns `None` for empty input. For a
/// single selection, the sole goal is the winner with an empty losers list. Pure and
/// stateless — reads `fixture` only to resolve tensions for each goal.
/// Delegates to `arbitrate_with_mode(.., Mode::Neutral)` — one sort implementation.
pub fn arbitrate(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
) -> Option<ArbitrationResult> {
    arbitrate_with_mode(selections, fixture, Mode::Neutral).map(|mode_result| {
        let winner_tier = mode_result.winner_bias.effective_tier;
        let winner_tension_id = mode_result.winner_effective_tension_id.clone();
        let losers = mode_result
            .losers
            .into_iter()
            .map(|l| {
                let reason = format!(
                    "tier {} lost to winner at tier {} ({})",
                    l.bias.effective_tier, winner_tier, winner_tension_id
                );
                ArbitrationLoser {
                    selection: l.selection,
                    effective_tier: l.bias.effective_tier,
                    effective_tension_id: l.effective_tension_id,
                    effective_tension_title: l.effective_tension_title,
                    reason,
                }
            })
            .collect();
        ArbitrationResult {
            winner: mode_result.winner,
            winner_effective_tier: winner_tier,
            winner_effective_tension_id: mode_result.winner_effective_tension_id,
            winner_effective_tension_title: mode_result.winner_effective_tension_title,
            losers,
        }
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

/// Compute the `BiasOutcome` for one goal given its effective tier and the active bias vector.
fn compute_bias_outcome(
    effective_tier: u8,
    tension_id: &str,
    bias_vector: &BTreeMap<String, i8>,
) -> BiasOutcome {
    if effective_tier <= PROTECTED_TIER_FLOOR {
        BiasOutcome {
            effective_tier,
            bias_applied: 0,
            biased_tier: effective_tier,
            protected: true,
        }
    } else {
        let bias_applied = bias_vector.get(tension_id).copied().unwrap_or(0);
        let raw = effective_tier as i16 + bias_applied as i16;
        let biased_tier = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8;
        BiasOutcome {
            effective_tier,
            bias_applied,
            biased_tier,
            protected: false,
        }
    }
}

/// Mode-aware cross-goal arbitration. Sort key: `(biased_tier asc, base_priority desc,
/// goal_id asc)`. Band goals are clamped so `biased_tier >= PROTECTED_TIER_FLOOR + 1`,
/// ensuring floor goals always sort ahead. Returns `None` for empty input.
/// `arbitrate` delegates here with `Mode::Neutral`, producing identical results.
pub fn arbitrate_with_mode(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
    mode: Mode,
) -> Option<ModeArbitrationResult> {
    if selections.is_empty() {
        return None;
    }

    let bias_vector = mode.bias_vector();

    let mut with_bias: Vec<(GoalSelection, String, String, BiasOutcome)> = selections
        .into_iter()
        .map(|selection| {
            let (effective_tier, tension_id, tension_title) =
                effective_tension_for_goal(&selection.goal, fixture);
            let bias = compute_bias_outcome(effective_tier, &tension_id, &bias_vector);
            (selection, tension_id, tension_title, bias)
        })
        .collect();

    // Sort: biased_tier ascending, base_priority descending, goal_id ascending.
    with_bias.sort_by(|a, b| {
        a.3.biased_tier
            .cmp(&b.3.biased_tier)
            .then(b.0.goal.base_priority.cmp(&a.0.goal.base_priority))
            .then(a.0.goal.id.cmp(&b.0.goal.id))
    });

    let (winner_sel, winner_tension_id, winner_tension_title, winner_bias) = with_bias.remove(0);

    let winner_biased_tier = winner_bias.biased_tier;
    let winner_tid = winner_tension_id.clone();
    let losers = with_bias
        .into_iter()
        .map(|(sel, tension_id, tension_title, bias)| {
            let reason = format!(
                "biased tier {} (effective {}) lost to winner at biased tier {} ({})",
                bias.biased_tier, bias.effective_tier, winner_biased_tier, winner_tid
            );
            ModeArbitrationLoser {
                selection: sel,
                effective_tension_id: tension_id,
                effective_tension_title: tension_title,
                bias,
                reason,
            }
        })
        .collect();

    Some(ModeArbitrationResult {
        mode,
        winner: winner_sel,
        winner_effective_tension_id: winner_tension_id,
        winner_effective_tension_title: winner_tension_title,
        winner_bias,
        losers,
    })
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

/// A goal candidate proposed by a reflection step. Stays in `VolitionState::pending_candidates`
/// until explicitly accepted or rejected. Cannot be constructed with an empty
/// `proposal_evidence` — use `try_new`.
///
/// `activation_keywords` are derived at proposal time from the matched tension id parts
/// (e.g. `continuity-preservation` → `["continuity", "preservation"]`) so the accepted goal
/// can compete in `select_goals_with_salience` without requiring callers to supply keywords.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProposedGoalCandidate {
    id: String,
    title: String,
    summary: String,
    tension_ids: Vec<String>,
    scope: GoalScope,
    base_priority: u8,
    allowed_effects: Vec<AllowedEffect>,
    satisfaction_condition_summary: String,
    proposal_evidence: Vec<EvidenceRef>,
    source_description: String,
    activation_keywords: Vec<String>,
}

impl ProposedGoalCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        id: String,
        title: String,
        summary: String,
        tension_ids: Vec<String>,
        scope: GoalScope,
        base_priority: u8,
        allowed_effects: Vec<AllowedEffect>,
        satisfaction_condition_summary: String,
        proposal_evidence: Vec<EvidenceRef>,
        source_description: String,
        activation_keywords: Vec<String>,
    ) -> Result<Self, &'static str> {
        if proposal_evidence.is_empty() {
            return Err("proposal_evidence must not be empty");
        }
        Ok(Self {
            id,
            title,
            summary,
            tension_ids,
            scope,
            base_priority,
            allowed_effects,
            satisfaction_condition_summary,
            proposal_evidence,
            source_description,
            activation_keywords,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn proposal_evidence(&self) -> &[EvidenceRef] {
        &self.proposal_evidence
    }

    pub fn tension_ids(&self) -> &[String] {
        &self.tension_ids
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn activation_keywords(&self) -> &[String] {
        &self.activation_keywords
    }

    pub(crate) fn into_goal(self, acceptance_evidence: EvidenceRef) -> Goal {
        let mut evidence_refs: Vec<String> = self
            .proposal_evidence
            .iter()
            .map(|e| e.to_string())
            .collect();
        evidence_refs.push(acceptance_evidence.to_string());
        Goal {
            id: self.id,
            title: self.title,
            summary: self.summary,
            tension_ids: self.tension_ids,
            status: GoalStatus::Accepted,
            scope: self.scope,
            base_priority: self.base_priority,
            activation_keywords: self.activation_keywords,
            allowed_effects: self.allowed_effects,
            satisfaction_condition_summary: self.satisfaction_condition_summary,
            evidence_refs,
            estimated_tokens: 20,
            source_reference: self.source_description,
        }
    }
}

/// Shadow struct used only for deserialization; validates via `ProposedGoalCandidate::try_new`
/// so that the non-empty `proposal_evidence` invariant is enforced even through serde.
#[derive(Deserialize)]
struct ProposedGoalCandidateRaw {
    id: String,
    title: String,
    summary: String,
    tension_ids: Vec<String>,
    scope: GoalScope,
    base_priority: u8,
    allowed_effects: Vec<AllowedEffect>,
    satisfaction_condition_summary: String,
    proposal_evidence: Vec<EvidenceRef>,
    source_description: String,
    #[serde(default)]
    activation_keywords: Vec<String>,
}

impl<'de> Deserialize<'de> for ProposedGoalCandidate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ProposedGoalCandidateRaw::deserialize(deserializer)?;
        Self::try_new(
            raw.id,
            raw.title,
            raw.summary,
            raw.tension_ids,
            raw.scope,
            raw.base_priority,
            raw.allowed_effects,
            raw.satisfaction_condition_summary,
            raw.proposal_evidence,
            raw.source_description,
            raw.activation_keywords,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Result of `propose_goal_candidates`: matched candidates and questions that matched no
/// tension (for caller inspection without needing to infer from count differences).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalCandidateProposalResult {
    pub candidates: Vec<ProposedGoalCandidate>,
    pub unmatched_questions: Vec<String>,
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
    /// The most recent initiative output for this goal, set by `InitiativeExecuted`.
    pub last_initiative_output: Option<InitiativeOutput>,
}

impl GoalDynamicState {
    pub fn initial() -> Self {
        Self {
            status: GoalStatus::Accepted,
            salience: 0,
            reinforcement_count: 0,
            progress_evidence_refs: Vec::new(),
            last_activated_tick: None,
            last_satisfied_tick: None,
            cooldown_until_tick: None,
            last_initiative_output: None,
        }
    }
}

/// Durable-within-a-run volition state: a logical tick and per-goal dynamic state for
/// all Accepted goals seeded from the fixture.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionState {
    pub tick: u64,
    /// Keyed by goal id. Holds dynamic state for fixture-seeded goals and accepted candidates
    /// (wired into the selector and lifecycle reducer after `GoalCandidateAccepted`).
    pub goals: BTreeMap<String, GoalDynamicState>,
    /// Proposed goal candidates awaiting explicit accept or reject.
    pub pending_candidates: Vec<ProposedGoalCandidate>,
    /// Accepted goal data records keyed by goal id. Distinct from `goals`; holds the static
    /// `Goal` struct (title, tension_ids, activation_keywords, etc.) for accepted candidates.
    pub accepted_candidates: BTreeMap<String, Goal>,
    /// Active arbitration bias mode. Changed via `ModeChanged` event; default `Neutral`.
    #[serde(default)]
    pub mode: Mode,
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
        Self {
            tick: 0,
            goals,
            pending_candidates: Vec::new(),
            accepted_candidates: BTreeMap::new(),
            mode: Mode::Neutral,
        }
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
    /// Appends a proposed goal candidate to `pending_candidates`. Does not auto-accept.
    GoalCandidateAdded {
        candidate: ProposedGoalCandidate,
        tick: u64,
    },
    /// Moves a pending candidate to `accepted_candidates`. No-op if the candidate id is
    /// not in `pending_candidates`.
    GoalCandidateAccepted {
        goal_id: String,
        acceptance_evidence: EvidenceRef,
        tick: u64,
    },
    /// Removes a pending candidate from `pending_candidates`. Rejection reason is
    /// captured in the event log; no durable state for rejected candidates is kept.
    GoalCandidateRejected {
        goal_id: String,
        reason: String,
        tick: u64,
    },
    /// Records a bounded internal initiative output. Sets the goal to Active and stores
    /// the output in `GoalDynamicState::last_initiative_output`. Executes no external effect.
    InitiativeExecuted {
        goal_id: String,
        effect: AllowedEffect,
        output: InitiativeOutput,
        rationale: String,
        tick: u64,
    },
    /// Sets the active arbitration bias mode via the pure reducer. Replayable and traceable.
    ModeChanged {
        mode: Mode,
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
        VolitionEvent::GoalCandidateAdded { candidate, .. } => {
            state.pending_candidates.push(candidate);
        }
        VolitionEvent::GoalCandidateAccepted {
            goal_id,
            acceptance_evidence,
            ..
        } => {
            if let Some(pos) = state
                .pending_candidates
                .iter()
                .position(|c| c.id() == goal_id)
            {
                let candidate = state.pending_candidates.remove(pos);
                let goal = candidate.into_goal(acceptance_evidence);
                // Insert initial dynamic state so the accepted goal participates in
                // select_goals_with_salience with the same salience/cooldown paths as
                // fixture goals.
                state
                    .goals
                    .entry(goal_id.clone())
                    .or_insert_with(GoalDynamicState::initial);
                state.accepted_candidates.insert(goal_id, goal);
            }
        }
        VolitionEvent::GoalCandidateRejected { goal_id, .. } => {
            state.pending_candidates.retain(|c| c.id() != goal_id);
        }
        VolitionEvent::InitiativeExecuted {
            goal_id,
            output,
            tick,
            ..
        } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Active;
                dynamic.last_activated_tick = Some(tick);
                dynamic.last_initiative_output = Some(output);
            }
        }
        VolitionEvent::ModeChanged { mode, .. } => {
            state.mode = mode;
        }
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
        | VolitionEvent::TickAdvanced { tick }
        | VolitionEvent::GoalCandidateAdded { tick, .. }
        | VolitionEvent::GoalCandidateAccepted { tick, .. }
        | VolitionEvent::GoalCandidateRejected { tick, .. }
        | VolitionEvent::InitiativeExecuted { tick, .. }
        | VolitionEvent::ModeChanged { tick, .. } => *tick,
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

/// Map open questions to `ProposedGoalCandidate` values by matching question terms against
/// tension ids and summaries. Pure and deterministic — no model call. Questions that match
/// no tension are collected in `unmatched_questions`.
pub fn propose_goal_candidates(
    open_questions: &[String],
    fixture: &VolitionFixture,
) -> GoalCandidateProposalResult {
    let mut candidates = Vec::new();
    let mut unmatched_questions = Vec::new();

    for question in open_questions {
        let question_terms = normalize_terms(question);
        let matched_tension_ids: Vec<String> = fixture
            .tensions
            .iter()
            .filter(|tension| tension_matches_question(tension, &question_terms))
            .map(|tension| tension.id.clone())
            .collect();

        if matched_tension_ids.is_empty() {
            unmatched_questions.push(question.clone());
            continue;
        }

        let trimmed = question.trim();
        let evidence = EvidenceRef::try_new(format!("open-question: {trimmed}"))
            .expect("trimmed question is non-empty; construction cannot fail");
        let id = question_to_slug(trimmed);

        // Derive activation keywords from matched tension id parts so the accepted
        // goal can compete in select_goals_with_salience without extra caller input.
        let activation_keywords: Vec<String> = matched_tension_ids
            .iter()
            .flat_map(|tension_id| tension_id.split('-').map(str::to_lowercase))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        let candidate = ProposedGoalCandidate::try_new(
            id,
            trimmed.to_string(),
            trimmed.to_string(),
            matched_tension_ids,
            GoalScope::Session,
            70,
            vec![AllowedEffect::Reflect],
            format!("The question '{trimmed}' is resolved or addressed."),
            vec![evidence],
            format!("open-question: {trimmed}"),
            activation_keywords,
        )
        .expect("evidence is non-empty; construction cannot fail");

        candidates.push(candidate);
    }

    GoalCandidateProposalResult {
        candidates,
        unmatched_questions,
    }
}

fn tension_matches_question(tension: &Tension, question_terms: &[String]) -> bool {
    let id_terms: Vec<String> = tension.id.split('-').map(str::to_lowercase).collect();
    let summary_terms = normalize_terms(&tension.summary);
    question_terms.iter().any(|term| {
        id_terms.iter().any(|id_term| id_term == term)
            || summary_terms.iter().any(|s_term| s_term == term)
    })
}

fn question_to_slug(question: &str) -> String {
    let slug: String = question
        .chars()
        .take(50)
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push(c);
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    format!("proposed-{}", result.trim_matches('-'))
}

/// Map an `InitiativeProposal` to an `InitiativeOutput`. Pure and deterministic — no model
/// call. Maps `AllowedEffect` to the corresponding output variant using goal fields and
/// `initiative.matched_terms`.
pub fn execute_initiative(initiative: &InitiativeProposal, goal: &Goal) -> InitiativeOutput {
    match initiative.effect {
        AllowedEffect::Reflect => InitiativeOutput::ReflectionRequested {
            proposed_question: format!("Open question for goal '{}': {}", goal.title, goal.summary),
        },
        AllowedEffect::RetrieveContext => InitiativeOutput::ContextRetrievalRequested {
            query_terms: initiative.matched_terms.clone(),
        },
        AllowedEffect::ProposeExperiment => InitiativeOutput::ExperimentProposed {
            hypothesis: format!(
                "Experiment hypothesis for '{}': {}",
                goal.title, goal.summary
            ),
            scope: goal.scope,
        },
        AllowedEffect::SurfaceOpenThread => InitiativeOutput::OpenThreadSurfaced {
            thread_summary: goal.summary.clone(),
        },
    }
}

/// Normalize input text into lowercase word tokens, deduplicated, in order of first
/// appearance. Used by goal selection and candidate proposal matching.
pub fn normalize_terms(input: &str) -> Vec<String> {
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
    use super::*;

    // ── Test helpers ────────────────────────────────────────────────────────

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

    // ── EvidenceRef ─────────────────────────────────────────────────────────

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

    // ── Reducer determinism ─────────────────────────────────────────────────

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

    // ── Goal lifecycle reducers ─────────────────────────────────────────────

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

    // ── Arbitration ─────────────────────────────────────────────────────────

    #[test]
    fn arbitrate_empty_returns_none() {
        let fixture = static_fixture();
        assert!(arbitrate(vec![], &fixture).is_none());
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

    // ── Mode floor immunity ─────────────────────────────────────────────────

    #[test]
    fn floor_goal_wins_over_band_goals_under_exploratory_mode() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("floor-tension", 1),
                make_tension("band-tension", 7),
            ],
            goals: vec![],
        };
        let floor_goal =
            make_goal_for_arbitration("floor-goal", vec!["floor-tension".to_string()], 80);
        let band_goal =
            make_goal_for_arbitration("band-goal", vec!["band-tension".to_string()], 95);

        // Under Exploratory, band-tension would normally be promoted; floor-tension must be immune.
        let result =
            arbitrate_with_mode(vec![floor_goal, band_goal], &fixture, Mode::Exploratory).unwrap();

        assert_eq!(result.winner.goal.id, "floor-goal");
        assert!(result.winner_bias.protected, "floor goal must be protected");
        assert_eq!(
            result.winner_bias.biased_tier, result.winner_bias.effective_tier,
            "protected goal must have no bias applied"
        );
        for loser in &result.losers {
            if !loser.bias.protected {
                assert!(
                    loser.bias.biased_tier > PROTECTED_TIER_FLOOR,
                    "band goal biased_tier must not enter the protected floor"
                );
            }
        }
    }

    #[test]
    fn floor_goal_wins_under_all_modes() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("tier-2-tension", 2),
                make_tension("tier-7-tension", 7),
            ],
            goals: vec![],
        };
        let floor_goal =
            make_goal_for_arbitration("floor-goal", vec!["tier-2-tension".to_string()], 70);
        let band_goal =
            make_goal_for_arbitration("band-goal", vec!["tier-7-tension".to_string()], 99);

        for mode in [Mode::Neutral, Mode::Focused, Mode::Exploratory] {
            let result =
                arbitrate_with_mode(vec![floor_goal.clone(), band_goal.clone()], &fixture, mode)
                    .unwrap();
            assert_eq!(
                result.winner.goal.id, "floor-goal",
                "floor goal must win under {mode}"
            );
            assert!(
                result.winner_bias.protected,
                "floor goal must be protected under {mode}"
            );
        }
    }

    // ── ProposedGoalCandidate ────────────────────────────────────────────────

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

    // ── Accepted-candidate selector wiring (reducer side) ───────────────────

    #[test]
    fn accepted_candidate_goal_data_in_accepted_candidates_dynamic_state_in_goals() {
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

    // ── propose_goal_candidates ──────────────────────────────────────────────────

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

    // ── Initiative output stability ─────────────────────────────────────────

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

    // ── Mode bias vectors ───────────────────────────────────────────────────

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
        let effective_tier: u8 = 5;
        let bias_applied: i8 = i8::MIN; // -128, extreme promotion attempt
        let raw = effective_tier as i16 + bias_applied as i16; // 5 - 128 = -123
        let biased_tier = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8;
        assert_eq!(biased_tier, PROTECTED_TIER_FLOOR + 1);

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
    fn mode_field_serde_default_is_neutral() {
        let json = serde_json::json!({
            "tick": 0,
            "goals": {},
            "pending_candidates": [],
            "accepted_candidates": {}
        });
        let state: VolitionState = serde_json::from_value(json).unwrap();
        assert_eq!(state.mode, Mode::Neutral);
    }

    // ── No dependency on qsf_app ────────────────────────────────────────────

    #[test]
    fn static_fixture_loads_and_is_deterministic() {
        let f1 = static_fixture();
        let f2 = static_fixture();
        assert_eq!(f1, f2);
        assert!(!f1.tensions.is_empty());
        assert!(!f1.goals.is_empty());
    }
}
