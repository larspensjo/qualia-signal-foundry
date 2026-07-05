use super::super::{
    AllowedEffect, EvidenceRef, GoalScope, GoalStatus, ProposedGoalCandidate,
    SALIENCE_ACTIVATION_BONUS, VolitionEvent, VolitionState, apply, propose_goal_candidates,
    static_fixture,
};
use crate::context::ContextBudget;

// ── Phase 6: ProposedGoalCandidate ─────────────────────────────────────────

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
            coherence_decline: None,
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

#[test]
fn accepted_candidate_goal_data_in_accepted_candidates_dynamic_state_in_goals() {
    // Goal data (the Goal struct) lives in accepted_candidates.
    // Dynamic state (GoalDynamicState) lives in state.goals — same map as fixture
    // goals — so the accepted candidate participates in selector and lifecycle events.
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
fn accepted_candidate_with_no_keywords_does_not_appear_in_selector() {
    // Phase 7: accepted candidates are wired into the selector, but a candidate
    // with empty tension_ids derives no activation_keywords and never matches.
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let candidate = make_candidate("cand-selector");

    let state = apply(
        state,
        VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
    );
    let acceptance_evidence = EvidenceRef::try_new("trace-1").unwrap();
    let state = apply(
        state,
        VolitionEvent::GoalCandidateAccepted {
            goal_id: "cand-selector".to_string(),
            acceptance_evidence,
            tick: 2,
        },
    );

    let result = super::super::select_goals_with_salience(
        "cand selector",
        &fixture,
        &state,
        ContextBudget::new(4, 200),
    );
    assert!(
        result.selected.iter().all(|s| s.goal.id != "cand-selector"),
        "candidate with no activation keywords must not appear in selector output"
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

// ── Phase 7: Selector wiring ──────────────────────────────────────────────

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
fn accepted_candidate_with_derived_keywords_appears_in_selector() {
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
    let evidence = EvidenceRef::try_new("trace: accepted for selector wiring test").unwrap();
    let state = apply(
        state,
        VolitionEvent::GoalCandidateAccepted {
            goal_id: candidate_id.clone(),
            acceptance_evidence: evidence,
            tick: 2,
        },
    );

    // "continuity" matches keywords derived from "continuity-preservation"
    let result = super::super::select_goals_with_salience(
        "Is continuity still preserved?",
        &fixture,
        &state,
        ContextBudget::new(4, 200),
    );

    assert!(
        result.selected.iter().any(|s| s.goal.id == candidate_id),
        "accepted candidate must appear in selector output when input matches its derived keywords"
    );
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
