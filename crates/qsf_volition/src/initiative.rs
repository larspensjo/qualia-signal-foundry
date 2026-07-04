use serde::{Deserialize, Serialize};

use crate::{ActivationKeyword, AllowedEffect, Goal, GoalScope, TensionPriority};

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
    pub matched_keywords: Vec<ActivationKeyword>,
    pub match_strength: u32,
    pub initiative: InitiativeProposal,
}

impl GoalSelection {
    /// The matched keywords' terms, for readers that only need the strings (e.g. context
    /// fragment tags, trace evidence). The weighted form lives in `matched_keywords`.
    pub fn matched_terms(&self) -> Vec<String> {
        self.matched_keywords
            .iter()
            .map(|keyword| keyword.term.clone())
            .collect()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::static_fixture;

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
}
