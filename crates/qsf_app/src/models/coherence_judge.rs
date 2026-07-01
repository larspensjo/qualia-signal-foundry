use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runtime::run_context::RunContext;
use crate::volition::{CoherenceJudgeRef, CoherenceVerdict, Contradiction};

use super::model_client::{ModelClient, ModelMessage, ModelRequest, invoke_model_role};
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

/// Rejects a verdict's contradictions with context when any pair names an id outside the
/// queried `goal_set` or contradicts a goal with itself. Phase-1 traces are meant to be
/// reconstructable from recorded facts alone, so a judge that names an unknown or
/// self-contradicting id must fail loudly here rather than let the pure resolvers silently
/// tier an unknown id as `u8::MAX` or the reducer no-op an unreal retirement.
fn validate_contradictions(
    goal_set: &[CoherenceJudgeGoalRef],
    contradictions: &[Contradiction],
) -> anyhow::Result<()> {
    let known_ids: std::collections::BTreeSet<&str> =
        goal_set.iter().map(|goal| goal.id.as_str()).collect();
    for contradiction in contradictions {
        anyhow::ensure!(
            contradiction.goal_a != contradiction.goal_b,
            "coherence judge returned a self-contradiction for goal `{}`",
            contradiction.goal_a
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

/// Detects contradictions within a goal set. Implementations only *detect* — resolution is
/// pure (`qsf_volition::coherence::resolve_admission` / `resolve_sweep`). One primitive
/// serves both triggers: admission passes `{existing goals + one candidate}`; a sweep passes
/// the whole evaluated goal set.
pub trait CoherenceJudge {
    fn judge(
        &self,
        context: &mut RunContext,
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
        _context: &mut RunContext,
        goal_set: &[CoherenceJudgeGoalRef],
    ) -> anyhow::Result<CoherenceVerdict> {
        let ids: std::collections::BTreeSet<&str> =
            goal_set.iter().map(|goal| goal.id.as_str()).collect();
        let contradictions: Vec<Contradiction> = self
            .scripted_pairs
            .iter()
            .filter(|(a, b, _)| ids.contains(a.as_str()) && ids.contains(b.as_str()))
            .map(|(a, b, rationale)| Contradiction {
                goal_a: a.clone(),
                goal_b: b.clone(),
                rationale: rationale.clone(),
            })
            .collect();
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
        context: &mut RunContext,
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

        let response = invoke_model_role(context, self.client, &request)?;
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
    use crate::models::MockModelClient;

    fn goal(id: &str) -> CoherenceJudgeGoalRef {
        CoherenceJudgeGoalRef::new(id, format!("{id} title"), format!("{id} summary"))
    }

    #[test]
    fn scripted_judge_returns_contradiction_only_when_both_ids_are_queried() {
        let judge = ScriptedCoherenceJudge::new(vec![(
            "goal-a".to_string(),
            "goal-b".to_string(),
            "conflict".to_string(),
        )]);
        let base_dir = std::env::temp_dir().join(format!("qsf-coherence-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "coherence-judge-test").unwrap();

        let verdict = judge
            .judge(&mut context, &[goal("goal-a"), goal("goal-b")])
            .unwrap();
        assert_eq!(verdict.contradictions.len(), 1);
        assert_eq!(verdict.contradictions[0].goal_a, "goal-a");
        assert_eq!(verdict.contradictions[0].goal_b, "goal-b");

        let verdict_partial = judge.judge(&mut context, &[goal("goal-a")]).unwrap();
        assert!(verdict_partial.contradictions.is_empty());

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn model_backed_judge_parses_empty_contradictions_from_mock_client() {
        let client = MockModelClient::default();
        let judge = ModelBackedCoherenceJudge::new(&client);
        let base_dir = std::env::temp_dir().join(format!("qsf-coherence-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "coherence-judge-model-test").unwrap();

        let verdict = judge
            .judge(&mut context, &[goal("goal-a"), goal("goal-b")])
            .unwrap();

        assert!(verdict.contradictions.is_empty());
        assert_eq!(verdict.judge_ref.model_role, "coherence_judge");

        std::fs::remove_dir_all(base_dir).unwrap();
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
        let base_dir = std::env::temp_dir().join(format!("qsf-coherence-{}", uuid::Uuid::new_v4()));
        let mut context =
            RunContext::create_in(&base_dir, "coherence-judge-unknown-id-test").unwrap();

        let error = judge
            .judge(&mut context, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("goal-not-in-query"));

        std::fs::remove_dir_all(base_dir).unwrap();
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
        let base_dir = std::env::temp_dir().join(format!("qsf-coherence-{}", uuid::Uuid::new_v4()));
        let mut context =
            RunContext::create_in(&base_dir, "coherence-judge-self-contradiction-test").unwrap();

        let error = judge
            .judge(&mut context, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("self-contradiction"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn scripted_judge_rejects_a_scripted_self_contradiction() {
        let judge = ScriptedCoherenceJudge::new(vec![(
            "goal-a".to_string(),
            "goal-a".to_string(),
            "typo in fixture".to_string(),
        )]);
        let base_dir = std::env::temp_dir().join(format!("qsf-coherence-{}", uuid::Uuid::new_v4()));
        let mut context =
            RunContext::create_in(&base_dir, "coherence-judge-scripted-self-test").unwrap();

        let error = judge
            .judge(&mut context, &[goal("goal-a"), goal("goal-b")])
            .unwrap_err();
        assert!(error.to_string().contains("self-contradiction"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }
}
