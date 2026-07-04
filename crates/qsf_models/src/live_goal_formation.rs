use std::collections::{BTreeSet, HashMap};

use anyhow::Context;
use serde::Deserialize;
use serde_json::json;

use qsf_volition::{CoherenceJudgeRef, CoherenceVerdict, Contradiction, ProposedGoalCandidate};

use super::coherence_judge::{
    CoherenceJudgeGoalRef, contradictions_from_scripted_pairs,
    validate_contradictions_against_known_ids,
};
use super::model_client::{ModelClient, ModelInvoker, ModelMessage, ModelRequest};
use super::model_role::{ModelRole, ModelRoleId};

const LIVE_GOAL_FORMATION_PROMPT_VERSION: &str = "v1";

/// Output of one combined formation-and-detection call: an optional newly-proposed goal
/// candidate, plus a `CoherenceVerdict` over the queried goal set extended with that candidate
/// (if one was proposed). Detection still returns the same `CoherenceVerdict` shape as
/// `CoherenceJudge`, so the offline goal-coherence engine's pure `resolve_admission` is reused
/// unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveGoalFormationOutcome {
    pub proposed_candidate: Option<ProposedGoalCandidate>,
    pub verdict: CoherenceVerdict,
}

/// Proposes a candidate goal from a trusted turn's transcript (or nothing) and detects any
/// contradictions between it and the existing goal set, in one call. Formation and detection are
/// folded together (per the 2026-07-01 DecisionLog entry) so a single cache-structured prompt
/// covers both concerns every trusted turn.
pub trait LiveGoalFormationJudge {
    fn form_and_detect(
        &self,
        invoker: &mut dyn ModelInvoker,
        goal_set: &[CoherenceJudgeGoalRef],
        turn_transcript: &str,
    ) -> anyhow::Result<LiveGoalFormationOutcome>;
}

fn known_ids_including_candidate<'a>(
    goal_set: &'a [CoherenceJudgeGoalRef],
    candidate: &'a Option<ProposedGoalCandidate>,
) -> BTreeSet<&'a str> {
    let mut ids: BTreeSet<&str> = goal_set.iter().map(|goal| goal.id.as_str()).collect();
    if let Some(candidate) = candidate {
        ids.insert(candidate.id());
    }
    ids
}

/// Rejects a proposed candidate whose id already names a goal in the queried `goal_set`.
/// Without this check, an id collision (accidental, or a model returning an existing goal's id)
/// would pass validation silently — `known_ids_including_candidate` just dedups it — and
/// `accepted_candidates.insert(goal_id, ..)` would then overwrite that goal's title/summary in
/// every future judge prompt. The judge cannot flag the conflict itself: a contradiction between
/// the candidate and the same-id goal is `goal_a == goal_b`, which
/// `validate_contradictions_against_known_ids` rejects as a self-contradiction, not a collision.
fn validate_candidate_id_is_new(
    goal_set: &[CoherenceJudgeGoalRef],
    candidate: &Option<ProposedGoalCandidate>,
) -> anyhow::Result<()> {
    let Some(candidate) = candidate else {
        return Ok(());
    };
    anyhow::ensure!(
        !goal_set.iter().any(|goal| goal.id == candidate.id()),
        "live goal formation proposed candidate id `{}` that already names an existing goal in \
         the queried goal set",
        candidate.id()
    );
    Ok(())
}

/// Deterministic judge driving fixture turns in the offline harness: `scripted_formations` maps
/// an exact turn transcript to the candidate it should propose (`None` entries and unlisted
/// transcripts both mean "no candidate formed"); `scripted_pairs` are contradiction triples
/// applied whenever both ids (goal or freshly-proposed candidate) are present in the query.
pub struct ScriptedLiveGoalFormationJudge {
    judge_ref: CoherenceJudgeRef,
    scripted_formations: HashMap<String, ProposedGoalCandidate>,
    scripted_pairs: Vec<(String, String, String)>,
}

impl ScriptedLiveGoalFormationJudge {
    pub fn new(
        scripted_formations: HashMap<String, ProposedGoalCandidate>,
        scripted_pairs: Vec<(String, String, String)>,
    ) -> Self {
        Self {
            judge_ref: CoherenceJudgeRef {
                model_role: ModelRoleId::LiveGoalFormationJudge.to_string(),
                prompt_version: format!("{LIVE_GOAL_FORMATION_PROMPT_VERSION}-scripted"),
            },
            scripted_formations,
            scripted_pairs,
        }
    }
}

impl LiveGoalFormationJudge for ScriptedLiveGoalFormationJudge {
    fn form_and_detect(
        &self,
        _invoker: &mut dyn ModelInvoker,
        goal_set: &[CoherenceJudgeGoalRef],
        turn_transcript: &str,
    ) -> anyhow::Result<LiveGoalFormationOutcome> {
        let proposed_candidate = self.scripted_formations.get(turn_transcript).cloned();
        validate_candidate_id_is_new(goal_set, &proposed_candidate)?;
        let known_ids = known_ids_including_candidate(goal_set, &proposed_candidate);

        let contradictions = contradictions_from_scripted_pairs(&known_ids, &self.scripted_pairs);
        validate_contradictions_against_known_ids(&known_ids, &contradictions)?;

        Ok(LiveGoalFormationOutcome {
            proposed_candidate,
            verdict: CoherenceVerdict {
                contradictions,
                judge_ref: self.judge_ref.clone(),
            },
        })
    }
}

#[derive(Deserialize)]
struct LiveGoalFormationResponse {
    #[serde(default)]
    proposed_candidate: Option<ProposedGoalCandidate>,
    contradictions: Vec<Contradiction>,
}

/// Model-backed judge over the `ModelRoleId::LiveGoalFormationJudge` role. The goal-set system+user
/// messages form the stable, cacheable prefix (`with_stable_prefix_message_count(2)`); the turn
/// transcript is the variable suffix appended every call.
pub struct ModelBackedLiveGoalFormationJudge<'a> {
    client: &'a dyn ModelClient,
}

impl<'a> ModelBackedLiveGoalFormationJudge<'a> {
    pub fn new(client: &'a dyn ModelClient) -> Self {
        Self { client }
    }

    /// Builds the stable, cacheable prefix (system instructions + goal-set JSON,
    /// `with_stable_prefix_message_count(2)`) shared by `form_and_detect` and
    /// `live_goal_formation_stable_prefix_hash`, so the two can never compute the prefix
    /// differently. This is the single cache-boundary mechanism
    /// (`ModelRequest::stable_prefix_hash`) — see `live_goal_formation_stable_prefix_hash` for
    /// why callers should not maintain a second one.
    fn stable_prefix_request(goal_set: &[CoherenceJudgeGoalRef]) -> ModelRequest {
        let role = ModelRole::predefined(ModelRoleId::LiveGoalFormationJudge);
        let system_message = format!(
            "You review one trusted turn of a live conversation against the simulation's current \
             goal set. First, decide whether the turn warrants proposing a new durable goal \
             candidate (or none). Then identify any contradictions among the listed goals plus \
             your proposed candidate, if any - two goals contradict when pursuing one would \
             undermine or conflict with the other. Respond only with JSON of the form \
             {{\"proposed_candidate\": null | <candidate>, \"contradictions\": [{{\"goal_a\": id, \
             \"goal_b\": id, \"rationale\": text}}]}}, where <candidate> is exactly this shape: \
             {candidate_schema}. Return null for proposed_candidate and an empty list for \
             contradictions when nothing is warranted.",
            candidate_schema = ProposedGoalCandidate::json_schema_hint(),
        );
        ModelRequest::new(
            role,
            vec![
                ModelMessage::system(system_message),
                ModelMessage::user(
                    serde_json::to_string(&json!({ "goals": goal_set })).unwrap_or_default(),
                ),
            ],
        )
        .with_stable_prefix_message_count(2)
    }
}

/// Hashes the stable, cacheable prefix `ModelBackedLiveGoalFormationJudge` sends for `goal_set`
/// (system instructions + goal-set JSON), via `ModelRequest::stable_prefix_hash()` — the single
/// cache-boundary mechanism callers should track for `cached_prefix_ref`/`prefix_cache_eligible`.
/// A separate goal-set-only hash previously existed here and diverged from what the request
/// actually sent (it excluded the system prompt, so a prompt edit would not invalidate it);
/// deriving from `ModelRequest::stable_prefix_hash()` instead means the tracked hash and the
/// request bytes can never disagree.
pub fn live_goal_formation_stable_prefix_hash(goal_set: &[CoherenceJudgeGoalRef]) -> String {
    ModelBackedLiveGoalFormationJudge::stable_prefix_request(goal_set)
        .stable_prefix_hash()
        .expect("stable_prefix_request always sets stable_prefix_message_count")
}

impl LiveGoalFormationJudge for ModelBackedLiveGoalFormationJudge<'_> {
    fn form_and_detect(
        &self,
        invoker: &mut dyn ModelInvoker,
        goal_set: &[CoherenceJudgeGoalRef],
        turn_transcript: &str,
    ) -> anyhow::Result<LiveGoalFormationOutcome> {
        let mut request = Self::stable_prefix_request(goal_set)
            .with_temperature(0.0)
            .with_max_output_tokens(900);
        request
            .messages
            .push(ModelMessage::user(turn_transcript.to_string()));

        let response = invoker.invoke(self.client, &request)?;
        let structured = response
            .structured_output
            .as_ref()
            .context("live goal formation response had no structured output")?;
        let parsed: LiveGoalFormationResponse = serde_json::from_value(structured.clone())
            .with_context(|| {
                format!(
                    "live goal formation response did not match the expected \
                     {{proposed_candidate, contradictions}} shape; raw structured output: \
                     {structured}"
                )
            })?;

        validate_candidate_id_is_new(goal_set, &parsed.proposed_candidate)?;
        let known_ids = known_ids_including_candidate(goal_set, &parsed.proposed_candidate);
        validate_contradictions_against_known_ids(&known_ids, &parsed.contradictions)?;

        Ok(LiveGoalFormationOutcome {
            proposed_candidate: parsed.proposed_candidate,
            verdict: CoherenceVerdict {
                contradictions: parsed.contradictions,
                judge_ref: CoherenceJudgeRef {
                    model_role: ModelRoleId::LiveGoalFormationJudge.to_string(),
                    prompt_version: LIVE_GOAL_FORMATION_PROMPT_VERSION.to_string(),
                },
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockModelClient;
    use crate::model_client::DirectModelInvoker;
    use qsf_volition::{AllowedEffect, EvidenceRef, GoalScope};

    fn goal(id: &str) -> CoherenceJudgeGoalRef {
        CoherenceJudgeGoalRef::new(id, format!("{id} title"), format!("{id} summary"))
    }

    fn candidate(id: &str) -> ProposedGoalCandidate {
        ProposedGoalCandidate::try_new(
            id.to_string(),
            format!("{id} title"),
            format!("{id} summary"),
            vec!["some-tension".to_string()],
            GoalScope::Session,
            5,
            vec![AllowedEffect::Reflect],
            "satisfied when discussed".to_string(),
            vec![EvidenceRef::try_new("turn transcript evidence").unwrap()],
            "formed from live discussion".to_string(),
            vec!["some".to_string(), "tension".to_string()],
        )
        .unwrap()
    }

    #[test]
    fn scripted_judge_proposes_nothing_for_unlisted_transcript() {
        let judge = ScriptedLiveGoalFormationJudge::new(HashMap::new(), vec![]);
        let mut invoker = DirectModelInvoker;

        let outcome = judge
            .form_and_detect(&mut invoker, &[goal("goal-a")], "unrelated turn")
            .unwrap();

        assert!(outcome.proposed_candidate.is_none());
        assert!(outcome.verdict.contradictions.is_empty());
    }

    #[test]
    fn scripted_judge_proposes_the_scripted_candidate_for_a_matching_transcript() {
        let mut formations = HashMap::new();
        formations.insert("let's pursue a new goal".to_string(), candidate("goal-new"));
        let judge = ScriptedLiveGoalFormationJudge::new(formations, vec![]);
        let mut invoker = DirectModelInvoker;

        let outcome = judge
            .form_and_detect(&mut invoker, &[goal("goal-a")], "let's pursue a new goal")
            .unwrap();

        assert_eq!(
            outcome.proposed_candidate.map(|c| c.id().to_string()),
            Some("goal-new".to_string())
        );
    }

    #[test]
    fn scripted_judge_detects_contradiction_between_proposed_candidate_and_existing_goal() {
        let mut formations = HashMap::new();
        formations.insert("let's pursue a new goal".to_string(), candidate("goal-new"));
        let judge = ScriptedLiveGoalFormationJudge::new(
            formations,
            vec![(
                "goal-new".to_string(),
                "goal-a".to_string(),
                "conflicts with the existing goal".to_string(),
            )],
        );
        let mut invoker = DirectModelInvoker;

        let outcome = judge
            .form_and_detect(&mut invoker, &[goal("goal-a")], "let's pursue a new goal")
            .unwrap();

        assert_eq!(outcome.verdict.contradictions.len(), 1);
        assert_eq!(outcome.verdict.contradictions[0].goal_a, "goal-new");
        assert_eq!(outcome.verdict.contradictions[0].goal_b, "goal-a");
    }

    #[test]
    fn scripted_judge_rejects_a_candidate_id_that_collides_with_an_existing_goal() {
        let mut formations = HashMap::new();
        formations.insert("collide".to_string(), candidate("goal-a"));
        let judge = ScriptedLiveGoalFormationJudge::new(formations, vec![]);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .form_and_detect(&mut invoker, &[goal("goal-a")], "collide")
            .unwrap_err();

        assert!(error.to_string().contains("goal-a"));
    }

    #[test]
    fn model_backed_judge_rejects_a_candidate_id_that_collides_with_an_existing_goal() {
        let client = MockModelClient::default().with_fixture(
            ModelRoleId::LiveGoalFormationJudge,
            json!({
                "proposed_candidate": {
                    "id": "goal-a",
                    "title": "goal-a title",
                    "summary": "goal-a summary",
                    "tension_ids": ["some-tension"],
                    "scope": "session",
                    "base_priority": 5,
                    "allowed_effects": ["reflect"],
                    "satisfaction_condition_summary": "satisfied when discussed",
                    "proposal_evidence": ["turn transcript evidence"],
                    "source_description": "formed from live discussion",
                    "activation_keywords": ["some", "tension"]
                },
                "contradictions": []
            })
            .to_string(),
        );
        let judge = ModelBackedLiveGoalFormationJudge::new(&client);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .form_and_detect(&mut invoker, &[goal("goal-a")], "a live turn transcript")
            .unwrap_err();

        assert!(error.to_string().contains("goal-a"));
    }

    #[test]
    fn stable_prefix_prompt_enumerates_the_candidate_schema() {
        // Regression for the live-judge parse failures: the system prompt must spell out the
        // candidate fields, not gesture at "{candidate fields}", or the model invents a shape
        // that fails deserialization. Anchored to the required fields the deserializer reads.
        let request = ModelBackedLiveGoalFormationJudge::stable_prefix_request(&[goal("goal-a")]);
        let system = request.messages[0].content.as_str();
        for field in [
            "tension_ids",
            "proposal_evidence",
            "allowed_effects",
            "satisfaction_condition_summary",
            "activation_keywords",
        ] {
            assert!(
                system.contains(field),
                "system prompt must document the `{field}` candidate field"
            );
        }
        assert!(
            !system.contains("{candidate fields}"),
            "the placeholder that caused the live parse failures must not return"
        );
    }

    #[test]
    fn malformed_candidate_error_includes_the_raw_structured_output() {
        // A candidate missing a required field (here `tension_ids`) is the live failure mode; the
        // error must carry the raw response so the failure diagnostic can explain what the model
        // actually returned rather than only naming the missing field.
        let client = MockModelClient::default().with_fixture(
            ModelRoleId::LiveGoalFormationJudge,
            json!({
                "proposed_candidate": {
                    "id": "goal-new",
                    "title": "goal-new title",
                    "summary": "goal-new summary"
                },
                "contradictions": []
            })
            .to_string(),
        );
        let judge = ModelBackedLiveGoalFormationJudge::new(&client);
        let mut invoker = DirectModelInvoker;

        let error = judge
            .form_and_detect(&mut invoker, &[goal("goal-a")], "a live turn transcript")
            .unwrap_err();

        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("raw structured output"),
            "error must label the raw response: {rendered}"
        );
        assert!(
            rendered.contains("goal-new"),
            "error must include the raw response body: {rendered}"
        );
    }

    #[test]
    fn model_backed_judge_sets_stable_prefix_boundary_at_two_messages() {
        let client = MockModelClient::default();
        let judge = ModelBackedLiveGoalFormationJudge::new(&client);
        let mut invoker = DirectModelInvoker;

        // The mock client fixture for LiveGoalFormationJudge returns no candidate and no
        // contradictions, so the default path is a no-op.
        let outcome = judge
            .form_and_detect(&mut invoker, &[goal("goal-a")], "a live turn transcript")
            .unwrap();

        assert!(outcome.proposed_candidate.is_none());
        assert!(outcome.verdict.contradictions.is_empty());
    }

    #[test]
    fn stable_prefix_hash_is_stable_across_different_transcripts() {
        let goal_set = [goal("goal-a"), goal("goal-b")];

        assert_eq!(
            live_goal_formation_stable_prefix_hash(&goal_set),
            live_goal_formation_stable_prefix_hash(&goal_set),
            "the same goal set must hash identically regardless of when it's computed"
        );
    }

    #[test]
    fn stable_prefix_hash_changes_when_the_goal_set_changes() {
        let hash_a = live_goal_formation_stable_prefix_hash(&[goal("goal-a")]);
        let hash_b = live_goal_formation_stable_prefix_hash(&[goal("goal-a"), goal("goal-b")]);

        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn stable_prefix_hash_matches_the_request_actually_sent() {
        let goal_set = [goal("goal-a")];
        let request = ModelBackedLiveGoalFormationJudge::stable_prefix_request(&goal_set)
            .with_temperature(0.0)
            .with_max_output_tokens(900);

        assert_eq!(
            live_goal_formation_stable_prefix_hash(&goal_set),
            request.stable_prefix_hash().unwrap(),
            "the standalone hash must equal the hash of the prefix actually sent on the request"
        );
    }
}
