use serde::{Deserialize, Serialize};

use crate::{
    AllowedEffect, EvidenceRef, Goal, GoalScope, GoalStatus, Tension, VolitionFixture,
    normalize_terms,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::static_fixture;

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
}
