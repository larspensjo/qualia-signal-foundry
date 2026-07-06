use std::collections::BTreeMap;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;

use qsf_volition::{
    CoherenceJudgeRef, CoherenceVerdict, Contradiction, Goal, GoalStatus, VolitionFixture,
    VolitionState,
};

use super::model_client::{ModelClient, ModelInvoker, ModelMessage, ModelRequest};
use super::model_role::{ModelRole, ModelRoleId};

const COHERENCE_JUDGE_PROMPT_VERSION: &str = "v1";

/// One goal or candidate description sent to the coherence judge: just enough to reason
/// about contradiction without leaking full context-assembly state into the prompt.
#[derive(Clone, Debug, Serialize)]
pub struct CoherenceJudgeGoalRef {
    pub id: String,
    pub title: String,
    pub summary: String,
}

impl CoherenceJudgeGoalRef {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            summary: summary.into(),
        }
    }
}

/// Builds the goal set queried by a coherence check: fixture goals plus accepted candidates,
/// excluding any goal retired in `state`. Shared by the offline coherence harness and the live
/// realtime loop so both query the judge over the same evaluated set.
pub fn coherence_judge_goal_set(
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> Vec<CoherenceJudgeGoalRef> {
    let mut merged: BTreeMap<&str, &Goal> = BTreeMap::new();
    for goal in &fixture.goals {
        merged.insert(goal.id.as_str(), goal);
    }
    for (id, goal) in &state.accepted_candidates {
        merged.insert(id.as_str(), goal);
    }
    merged
        .into_iter()
        .filter_map(|(id, goal)| {
            let dynamic = state.goals.get(id)?;
            if dynamic.status == GoalStatus::Retired {
                return None;
            }
            Some(CoherenceJudgeGoalRef::new(
                goal.id.clone(),
                goal.title.clone(),
                goal.summary.clone(),
            ))
        })
        .collect()
}

/// Rejects a verdict's contradictions with context when any pair names an id outside the
/// queried `goal_set` or contradicts a goal with itself. Coherence traces are meant to be
/// reconstructable from recorded facts alone, so a judge that names an unknown or
/// self-contradicting id must fail loudly here rather than let the pure resolvers silently
/// tier an unknown id as `u8::MAX` or the reducer no-op an unreal retirement.
fn validate_contradictions(
    goal_set: &[CoherenceJudgeGoalRef],
    contradictions: &[Contradiction],
) -> anyhow::Result<()> {
    let known_ids: std::collections::BTreeSet<&str> =
        goal_set.iter().map(|goal| goal.id.as_str()).collect();
    validate_contradictions_against_known_ids(&known_ids, contradictions)
}

/// Shared by `CoherenceJudge` (known ids = the queried goal set) and the live-goal-formation
/// judge (known ids = the queried goal set plus the just-proposed candidate's id, since
/// contradictions may name the candidate itself).
pub(crate) fn validate_contradictions_against_known_ids(
    known_ids: &std::collections::BTreeSet<&str>,
    contradictions: &[Contradiction],
) -> anyhow::Result<()> {
    for contradiction in contradictions {
        anyhow::ensure!(
            contradiction.goal_a != contradiction.goal_b,
            "coherence judge returned a self-contradiction for goal `{}`",
            contradiction.goal_a
        );
        // A blank rationale cannot ground a coherence decline: it would surface a bare
        // `coherence_decline` signal with empty evidence, violating the contract that every
        // signal points at reconstructable recorded facts. Reject it at the same boundary that
        // rejects unknown and self-contradicting ids.
        anyhow::ensure!(
            !contradiction.rationale.trim().is_empty(),
            "coherence judge returned a blank rationale for contradiction between `{}` and `{}`",
            contradiction.goal_a,
            contradiction.goal_b
        );
        for id in [&contradiction.goal_a, &contradiction.goal_b] {
            anyhow::ensure!(
                known_ids.contains(id.as_str()),
                "coherence judge returned unknown goal id `{id}`, not in the queried goal set"
            );
        }
    }
    Ok(())
}

/// Filters scripted `(goal_a, goal_b, rationale)` triples to the ones where both ids are
/// present in `known_ids`, producing `Contradiction`s. Shared by `ScriptedCoherenceJudge` and
/// `ScriptedLiveGoalFormationJudge`, whose scripted-pair filters were previously identical
/// copies.
pub(crate) fn contradictions_from_scripted_pairs(
    known_ids: &std::collections::BTreeSet<&str>,
    scripted_pairs: &[(String, String, String)],
) -> Vec<Contradiction> {
    scripted_pairs
        .iter()
        .filter(|(a, b, _)| known_ids.contains(a.as_str()) && known_ids.contains(b.as_str()))
        .map(|(a, b, rationale)| Contradiction {
            goal_a: a.clone(),
            goal_b: b.clone(),
            rationale: rationale.clone(),
        })
        .collect()
}

/// Detects contradictions within a goal set. Implementations only *detect* — resolution is
/// pure (`qsf_volition::coherence::resolve_admission` / `resolve_sweep`). One primitive
/// serves both triggers: admission passes `{existing goals + one candidate}`; a sweep passes
/// the whole evaluated goal set.
///
/// `judge` takes a `ModelInvoker` rather than any one observability type, so offline callers
/// (backed by `RunContext`) and the live realtime loop (backed by its own diagnostics) can both
/// drive the same judge implementations.
pub trait CoherenceJudge {
    fn judge(
        &self,
        invoker: &mut dyn ModelInvoker,
        goal_set: &[CoherenceJudgeGoalRef],
    ) -> anyhow::Result<CoherenceVerdict>;
}

/// Deterministic mock judge: contradictions are scripted `(goal_a, goal_b, rationale)`
/// triples, applied whenever both ids are present in the queried `goal_set`. Order- and
/// side-independent — `(a, b)` matches a query containing both regardless of which one is
/// the new candidate.
pub struct ScriptedCoherenceJudge {
    judge_ref: CoherenceJudgeRef,
    scripted_pairs: Vec<(String, String, String)>,
}

impl ScriptedCoherenceJudge {
    pub fn new(scripted_pairs: Vec<(String, String, String)>) -> Self {
        Self {
            judge_ref: CoherenceJudgeRef {
                model_role: ModelRoleId::CoherenceJudge.to_string(),
                prompt_version: format!("{COHERENCE_JUDGE_PROMPT_VERSION}-scripted"),
            },
            scripted_pairs,
        }
    }
}

impl CoherenceJudge for ScriptedCoherenceJudge {
    fn judge(
        &self,
        _invoker: &mut dyn ModelInvoker,
        goal_set: &[CoherenceJudgeGoalRef],
    ) -> anyhow::Result<CoherenceVerdict> {
        let ids: std::collections::BTreeSet<&str> =
            goal_set.iter().map(|goal| goal.id.as_str()).collect();
        let contradictions = contradictions_from_scripted_pairs(&ids, &self.scripted_pairs);
        validate_contradictions(goal_set, &contradictions)?;
        Ok(CoherenceVerdict {
            contradictions,
            judge_ref: self.judge_ref.clone(),
        })
    }
}

/// Model-backed judge over the `ModelRoleId::CoherenceJudge` role. Real-model opt-in per the
/// existing provider-selection boundary (`build_client`); the mock provider is the default.
pub struct ModelBackedCoherenceJudge<'a> {
    client: &'a dyn ModelClient,
}

impl<'a> ModelBackedCoherenceJudge<'a> {
    pub fn new(client: &'a dyn ModelClient) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
struct CoherenceJudgeResponse {
    contradictions: Vec<Contradiction>,
}

impl CoherenceJudge for ModelBackedCoherenceJudge<'_> {
    fn judge(
        &self,
        invoker: &mut dyn ModelInvoker,
        goal_set: &[CoherenceJudgeGoalRef],
    ) -> anyhow::Result<CoherenceVerdict> {
        let role = ModelRole::predefined(ModelRoleId::CoherenceJudge);
        let request = ModelRequest::new(
            role,
            vec![
                ModelMessage::system(
                    "Identify contradictions among the listed goals. Two goals contradict when \
                     pursuing one would undermine or conflict with the other. Respond only with \
                     JSON: {\"contradictions\": [{\"goal_a\": id, \"goal_b\": id, \"rationale\": \
                     text}]}. Return an empty list when nothing contradicts.",
                ),
                ModelMessage::user(serde_json::to_string(&json!({ "goals": goal_set }))?),
            ],
        )
        .with_temperature(0.0)
        .with_max_output_tokens(600);

        let response = invoker.invoke(self.client, &request)?;
        let structured = response
            .structured_output
            .as_ref()
            .context("coherence judge response had no structured output")?;
        let parsed: CoherenceJudgeResponse = serde_json::from_value(structured.clone())
            .context("coherence judge response did not match the expected contradictions shape")?;
        validate_contradictions(goal_set, &parsed.contradictions)?;

        Ok(CoherenceVerdict {
            contradictions: parsed.contradictions,
            judge_ref: CoherenceJudgeRef {
                model_role: ModelRoleId::CoherenceJudge.to_string(),
                prompt_version: COHERENCE_JUDGE_PROMPT_VERSION.to_string(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockModelClient;
    use crate::model_client::DirectModelInvoker;

    fn goal(id: &str) -> CoherenceJudgeGoalRef {
        CoherenceJudgeGoalRef::new(id, format!("{id} title"), format!("{id} summary"))
    }

    #[test]
    fn coherence_judge_goal_set_excludes_retired_goals() {
        use qsf_volition::{VolitionEvent, apply, realtime_seed_fixture};

        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let before = coherence_judge_goal_set(&state, &fixture);
        assert!(!before.is_empty());

        let some_goal_id = fixture.goals[0].id.clone();
        let state = apply(
            state,
            VolitionEvent::GoalRetired {
                goal_id: some_goal_id.clone(),
                tick: 1,
            },
        );
        let after = coherence_judge_goal_set(&state, &fixture);
        assert!(!after.iter().any(|goal| goal.id == some_goal_id));
        assert_eq!(after.len(), before.len() - 1);
    }

    #[test]
    fn scripted_judge_returns_contradiction_only_when_both_ids_are_queried() {
        let judge = ScriptedCoherenceJudge::new(vec![(
            "goal-a".to_string(),
            "goal-b".to_string(),
            "conflict".to_string(),
        )]);
        let mut invoker = DirectModelInvoker;

        let verdict = judge
            .judge(&mut invoker, &[goal("goal-a"), goal("goal-b")])
            .unwrap();
        assert_eq!(verdict.contradictions.len(), 1);
        assert_eq!(verdict.contradictions[0].goal_a, "goal-a");
        assert_eq!(verdict.contradictions[0].goal_b, "goal-b");

        let verdict_partial = judge.judge(&mut invoker, &[goal("goal-a")]).unwrap();
        assert!(verdict_partial.contradictions.is_empty());
    }

    #[test]
    fn model_backed_judge_parses_empty_contradictions_from_mock_client() {
        let client = MockModelClient::default();
        let judge = ModelBackedCoherenceJudge::new(&client);
        let mut invoker = DirectModelInvoker;

        let verdict = judge
            .judge(&mut invoker, &[goal("goal-a"), goal("goal-b")])
            .unwrap();

        assert!(verdict.contradictions.is_empty());
        assert_eq!(verdict.judge_ref.model_role, "coherence_judge");
    }

    #[test]
    fn model_backed_judge_rejects_contradiction_with_unknown_goal_id() {
        let client = MockModelClient::default().with_fixture(
            ModelRoleId::CoherenceJudge,
            json!({
                "contradictions": [
                    { "goal_a": "goal-a", "goal_b": "goal-not-in-query", "rationale": "hallucinated" }
                ]
            })
            .to_string(),
        );
        let judge = ModelBackedCoherenceJudge::new(&client);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .judge(&mut invoker, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("goal-not-in-query"));
    }

    #[test]
    fn model_backed_judge_rejects_self_contradiction() {
        let client = MockModelClient::default().with_fixture(
            ModelRoleId::CoherenceJudge,
            json!({
                "contradictions": [
                    { "goal_a": "goal-a", "goal_b": "goal-a", "rationale": "self" }
                ]
            })
            .to_string(),
        );
        let judge = ModelBackedCoherenceJudge::new(&client);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .judge(&mut invoker, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("self-contradiction"));
    }

    #[test]
    fn model_backed_judge_rejects_blank_rationale() {
        let client = MockModelClient::default().with_fixture(
            ModelRoleId::CoherenceJudge,
            json!({
                "contradictions": [
                    { "goal_a": "goal-a", "goal_b": "goal-b", "rationale": "   " }
                ]
            })
            .to_string(),
        );
        let judge = ModelBackedCoherenceJudge::new(&client);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .judge(&mut invoker, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("blank rationale"));
    }

    #[test]
    fn scripted_judge_rejects_a_blank_rationale() {
        let judge = ScriptedCoherenceJudge::new(vec![(
            "goal-a".to_string(),
            "goal-b".to_string(),
            String::new(),
        )]);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .judge(&mut invoker, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("blank rationale"));
    }

    #[test]
    fn scripted_judge_rejects_a_scripted_self_contradiction() {
        let judge = ScriptedCoherenceJudge::new(vec![(
            "goal-a".to_string(),
            "goal-a".to_string(),
            "typo in fixture".to_string(),
        )]);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .judge(&mut invoker, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("self-contradiction"));
    }
}
