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
    /// A pure request to consult the read-only external world corpus. The caller may add
    /// current-topic terms from the actual turn, but this domain output records the activation
    /// terms that grounded the request.
    WorldConsultationRequested {
        query_terms: Vec<WorldQueryTerm>,
    },
    ExperimentProposed {
        hypothesis: String,
        scope: GoalScope,
    },
    OpenThreadSurfaced {
        thread_summary: String,
    },
}

/// Provenance of a lexical term in a world-corpus consultation. The lack of an open-question
/// substrate is intentional and visible: v1 distinguishes goal activation from current topic.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldQueryTermSource {
    GoalActivation,
    CurrentTopic,
}

/// A consultation term with its derivation source, kept serializable for adapter traces.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldQueryTerm {
    pub term: String,
    pub source: WorldQueryTermSource,
}

/// Pure domain result for an explicitly current topic. The adapter uses `required_anchors`
/// only as a relevance gate; all query terms remain available to lexical ranking.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExplicitTopicWorldConsultation {
    pub initiative_output: InitiativeOutput,
    pub required_anchors: Vec<String>,
}

/// Returns a pure consultation request for an explicitly current named topic.
///
/// This deliberately requires both a current-information cue (such as `release` or `latest`)
/// and a concrete topic signal (a named entity or dotted version). It is therefore an escape
/// hatch for turns like "What do you think about the Grok 4.5 release?", rather than a search
/// request for every ordinary user turn. The realtime adapter remains responsible for deciding
/// whether and how to execute the external read.
pub fn explicit_topic_world_consultation_request(
    input: &str,
) -> Option<ExplicitTopicWorldConsultation> {
    let normalized_terms = crate::normalize_terms(input);
    let has_current_information_cue = normalized_terms
        .iter()
        .any(|term| is_current_information_cue(term));
    let versions = dotted_versions(input);
    let named_entity_candidates = input
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .enumerate()
        .filter_map(|(index, word)| {
            let normalized = word.to_ascii_lowercase();
            (word.chars().next().is_some_and(char::is_uppercase)
                && !is_generic_world_query_term(&normalized))
            .then_some((index, normalized))
        })
        .collect::<Vec<_>>();

    // Sentence-initial capitalization alone is ambiguous. Prefer later capitalized signals when
    // present ("Tell me about ... Grok"), but retain the initial candidate as a fallback so the
    // deliberately narrow detector's existing admission behavior does not broaden or contract.
    let mut required_anchors = named_entity_candidates
        .iter()
        .filter(|(index, _)| *index > 0)
        .map(|(_, term)| term.clone())
        .collect::<Vec<_>>();
    if required_anchors.is_empty() {
        required_anchors.extend(named_entity_candidates.into_iter().map(|(_, term)| term));
    }
    required_anchors.extend(versions.iter().cloned());
    required_anchors.dedup();

    if !has_current_information_cue || required_anchors.is_empty() {
        return None;
    }

    let mut terms = normalized_terms
        .into_iter()
        .filter(|term| term.len() >= 2 && !is_generic_world_query_term(term))
        .map(|term| WorldQueryTerm {
            term,
            source: WorldQueryTermSource::CurrentTopic,
        })
        .collect::<Vec<_>>();
    for version in versions {
        if !terms.iter().any(|term| term.term == version) {
            terms.push(WorldQueryTerm {
                term: version,
                source: WorldQueryTermSource::CurrentTopic,
            });
        }
    }

    (!terms.is_empty()).then_some(ExplicitTopicWorldConsultation {
        initiative_output: InitiativeOutput::WorldConsultationRequested { query_terms: terms },
        required_anchors,
    })
}

/// Whether a normalized term is generic query framing rather than topic substance.
pub fn is_generic_world_query_term(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "are"
            | "about"
            | "can"
            | "did"
            | "do"
            | "does"
            | "for"
            | "how"
            | "i"
            | "is"
            | "it"
            | "me"
            | "of"
            | "on"
            | "or"
            | "the"
            | "think"
            | "to"
            | "what"
            | "when"
            | "will"
            | "with"
            | "you"
            | "your"
    )
}

/// Whether a normalized term explicitly asks for current external information.
pub fn is_current_information_cue(term: &str) -> bool {
    matches!(
        term,
        "release"
            | "launch"
            | "update"
            | "announcement"
            | "announced"
            | "latest"
            | "recent"
            | "news"
            | "current"
            | "today"
            | "happened"
    )
}

/// Whether a term is exactly a two-component dotted numeric version such as `4.5`.
pub fn is_dotted_version(term: &str) -> bool {
    let mut parts = term.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !major.is_empty()
        && !minor.is_empty()
        && major.chars().all(|character| character.is_ascii_digit())
        && minor.chars().all(|character| character.is_ascii_digit())
}

fn dotted_versions(input: &str) -> Vec<String> {
    input
        .split(|character: char| character.is_whitespace() || character == '?')
        .filter_map(|word| {
            let version = word
                .trim_matches(|character: char| !character.is_ascii_digit() && character != '.');
            is_dotted_version(version).then_some(version.to_string())
        })
        .collect()
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
        AllowedEffect::ConsultWorld => InitiativeOutput::WorldConsultationRequested {
            query_terms: initiative
                .matched_terms
                .iter()
                .cloned()
                .map(|term| WorldQueryTerm {
                    term,
                    source: WorldQueryTermSource::GoalActivation,
                })
                .collect(),
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
    fn execute_initiative_consult_world_preserves_goal_activation_provenance() {
        let fixture = crate::realtime_seed_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|goal| goal.id == "assemble-world-picture")
            .unwrap();
        let initiative = InitiativeProposal {
            goal_id: goal.id.clone(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::ConsultWorld,
            rationale: "test".to_string(),
            matched_terms: vec!["world".to_string(), "trend".to_string()],
            scope: goal.scope,
        };

        let output = execute_initiative(&initiative, goal);

        assert_eq!(
            output,
            InitiativeOutput::WorldConsultationRequested {
                query_terms: vec![
                    WorldQueryTerm {
                        term: "world".to_string(),
                        source: WorldQueryTermSource::GoalActivation,
                    },
                    WorldQueryTerm {
                        term: "trend".to_string(),
                        source: WorldQueryTermSource::GoalActivation,
                    },
                ],
            }
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
    fn explicit_release_prompt_requests_a_current_topic_consultation() {
        let request = explicit_topic_world_consultation_request(
            "What do you think about the Grok 4.5 release?",
        )
        .expect("explicit release request");

        assert_eq!(
            request.initiative_output,
            InitiativeOutput::WorldConsultationRequested {
                query_terms: vec![
                    WorldQueryTerm {
                        term: "grok".to_string(),
                        source: WorldQueryTermSource::CurrentTopic,
                    },
                    WorldQueryTerm {
                        term: "release".to_string(),
                        source: WorldQueryTermSource::CurrentTopic,
                    },
                    WorldQueryTerm {
                        term: "4.5".to_string(),
                        source: WorldQueryTermSource::CurrentTopic,
                    },
                ],
            }
        );
        assert_eq!(request.required_anchors, ["grok", "4.5"]);
    }

    #[test]
    fn sentence_framing_is_not_promoted_to_an_explicit_topic_anchor() {
        let request =
            explicit_topic_world_consultation_request("Tell me about the latest Grok release")
                .expect("explicit release request");

        assert_eq!(request.required_anchors, ["grok"]);
        let InitiativeOutput::WorldConsultationRequested { query_terms } =
            request.initiative_output
        else {
            panic!("explicit topic helper only returns a consultation request");
        };
        assert!(query_terms.iter().any(|term| term.term == "tell"));
    }

    #[test]
    fn two_character_named_entity_is_retained_as_an_anchor() {
        let request =
            explicit_topic_world_consultation_request("Tell me about the latest VR release")
                .expect("explicit release request");

        assert_eq!(request.required_anchors, ["vr"]);
        let InitiativeOutput::WorldConsultationRequested { query_terms } =
            request.initiative_output
        else {
            panic!("explicit topic helper only returns a consultation request");
        };
        assert!(query_terms.iter().any(|term| term.term == "vr"));
    }

    #[test]
    fn ordinary_turn_does_not_become_a_world_consultation_request() {
        assert!(explicit_topic_world_consultation_request("How can you help me today?").is_none());
        assert!(explicit_topic_world_consultation_request("What do you think?").is_none());
    }
}
