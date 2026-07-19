use qsf_volition::{
    DeclineReason, DeclinedCandidate, Mode, ShapingIntensity, VolitionFixture, VolitionState,
    arbitrate_with_mode, detect_opportunities, grounded_terms_from_text, realtime_seed_fixture,
    select_goals_ranked,
};

use crate::realtime::tools::VolitionStateSnapshot;
use crate::realtime::volition_injection::{
    build_stable_baseline_instructions, build_volition_context_injection_trace,
    build_volition_turn_context_packet,
};

fn fixture_state() -> (VolitionFixture, VolitionState) {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    (fixture, state)
}

#[test]
fn stable_baseline_wraps_rendered_stance() {
    let fixture = realtime_seed_fixture();
    let baseline = build_stable_baseline_instructions(&fixture, Mode::Neutral);
    assert!(baseline.starts_with(
        "The following describes your own volition stance — part of your inner life."
    ));
    assert!(baseline.contains("Volition stance"));
    assert!(baseline.contains("never authorizes any action outside this"));
    let lowered = baseline.to_lowercase();
    assert!(!lowered.contains("not a claim"));
    assert!(!lowered.contains("simulat"));
}

#[test]
fn turn_packet_builder_returns_none_for_empty_selection() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    let ranked = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
    let opportunities = detect_opportunities(&grounded_terms_from_text("xyzzy"), &state, &fixture);
    assert!(
        build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            None,
            &opportunities,
            ShapingIntensity::None,
            "stable-baseline-hash".to_string(),
            None,
            &[],
        )
        .is_none()
    );
}

#[test]
fn serialized_trace_satisfies_the_weighted_activation_trace_contract() {
    // Recompute strength here from the wire weight-class strings — deliberately NOT via
    // KeywordWeightClass — so the artifact boundary is checked, not the Rust types.
    fn wire_weight(class: &str) -> u32 {
        match class {
            "weak" => 1,
            "normal" => 4,
            "strong" => 8,
            other => panic!("unexpected weight_class on the wire: {other}"),
        }
    }
    fn recompute_strength(matched_keywords: &serde_json::Value) -> u32 {
        matched_keywords
            .as_array()
            .expect("matched_keywords is an array")
            .iter()
            .map(|keyword| {
                wire_weight(
                    keyword["weight_class"]
                        .as_str()
                        .expect("weight_class is a string"),
                )
            })
            .sum()
    }

    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };

    let build_trace = |query: &str| {
        let ranked = select_goals_ranked(query, &state, &fixture);
        let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
        let packet = build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            outcome,
            &[],
            ShapingIntensity::None,
            "stable-baseline-hash".to_string(),
            None,
            &[],
        )
        .expect("both probes activate at least one goal");
        let trace = build_volition_context_injection_trace(
            "session-1",
            1,
            "transcript-ref",
            state.tick,
            Vec::new(),
            &packet,
            "response-create-ref",
        );
        serde_json::to_value(&trace).expect("trace serializes")
    };

    for (query, expect_winner) in [
        ("how can you help me", true),
        ("for what it's worth, thanks", false),
    ] {
        let trace = build_trace(query);

        // 1. Threshold is a positive integer.
        let threshold = trace["qualification_threshold"]
            .as_u64()
            .expect("qualification_threshold present");
        assert!(threshold > 0, "query: {query}");

        // 4/5. Winner presence and the lower_arbitration_rank guard.
        let has_arbitration = !trace["arbitration_result"].is_null();
        assert_eq!(has_arbitration, expect_winner, "query: {query}");

        let candidates = trace["omitted_or_suppressed_candidates"]
            .as_array()
            .expect("candidates array");
        let mut saw_below_threshold = false;
        for candidate in candidates {
            let category = candidate["reason_category"].as_str().unwrap();
            if category == "below_qualification_threshold" {
                saw_below_threshold = true;
                // 2. Recomputed strength matches, and is below the threshold.
                let recomputed = recompute_strength(&candidate["matched_keywords"]);
                assert_eq!(
                    recomputed,
                    candidate["match_strength"].as_u64().unwrap() as u32,
                    "query: {query}"
                );
                assert!((recomputed as u64) < threshold, "query: {query}");
            }
            if category == "lower_arbitration_rank" {
                assert!(
                    has_arbitration,
                    "lower_arbitration_rank without a winner: {query}"
                );
            }
        }

        // 3. selected_match_details recompute the same way.
        for detail in trace["selector_output"]["selected_match_details"]
            .as_array()
            .expect("selected_match_details array")
        {
            assert_eq!(
                recompute_strength(&detail["matched_keywords"]),
                detail["match_strength"].as_u64().unwrap() as u32,
                "query: {query}"
            );
        }

        // 4. On the no-qualifier turn, at least one below-threshold candidate exists.
        if !expect_winner {
            assert!(saw_below_threshold, "no-qualifier turn: {query}");
        }
    }

    // Contract scope: a trusted turn with no lexical activation at all emits no packet.
    let ranked = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    assert!(
        build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            outcome,
            &[],
            ShapingIntensity::None,
            "stable-baseline-hash".to_string(),
            None,
            &[],
        )
        .is_none()
    );
}

#[test]
fn unqualified_turn_emits_packet_with_below_threshold_categorization() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    let ranked = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    assert!(
        outcome.as_ref().unwrap().qualified.is_none(),
        "precondition: no qualifier"
    );
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        outcome,
        &[],
        ShapingIntensity::None,
        "stable-baseline-hash".to_string(),
        None,
        &[],
    )
    .expect("activated-but-unqualified turn must emit a packet");
    assert!(packet.summary.arbitration_result.is_none());
    assert!(!packet.summary.below_threshold_candidates.is_empty());
    for candidate in &packet.summary.below_threshold_candidates {
        assert_eq!(candidate.reason_category, "below_qualification_threshold");
        assert_ne!(candidate.reason_category, "lower_arbitration_rank");
        assert!(!candidate.matched_keywords.is_empty());
        assert!(candidate.match_strength < packet.summary.qualification_threshold);
    }
    assert!(packet.text.contains("No goal qualified"));
    assert!(!packet.text.contains("Active goal:"));
}

#[test]
fn winner_turn_still_records_below_threshold_candidates_and_threshold() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    // "how can you help me": serve-the-present-person qualifies (how+can+help = 6), while
    // learn-what-drives-this-person activates only on the weak "me" (strength 1).
    let ranked = select_goals_ranked("how can you help me", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        outcome,
        &[],
        ShapingIntensity::Low,
        "stable-baseline-hash".to_string(),
        None,
        &[],
    )
    .expect("qualified winner emits a packet");
    assert!(packet.summary.arbitration_result.is_some());
    assert_eq!(
        packet.summary.qualification_threshold,
        fixture.arbitration_qualification_threshold
    );
    assert!(!packet.summary.below_threshold_candidates.is_empty());
    for candidate in &packet.summary.below_threshold_candidates {
        assert_eq!(candidate.reason_category, "below_qualification_threshold");
        assert!(!candidate.matched_keywords.is_empty());
        assert_eq!(
            candidate.match_strength,
            candidate
                .matched_keywords
                .iter()
                .map(|k| k.weight())
                .sum::<u32>()
        );
    }
}

#[test]
fn selected_match_details_cover_every_selected_goal() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    let ranked = select_goals_ranked("how can you help me", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        outcome,
        &[],
        ShapingIntensity::Low,
        "stable-baseline-hash".to_string(),
        None,
        &[],
    )
    .expect("packet");
    let details = &packet.summary.selector_output.selected_match_details;
    assert_eq!(details.len(), ranked.selected.len());
    for detail in details {
        assert_eq!(
            detail.match_strength,
            detail
                .matched_keywords
                .iter()
                .map(|k| k.weight())
                .sum::<u32>()
        );
        assert!(ranked.selected.iter().any(|s| s.goal.id == detail.goal_id));
    }
}

#[test]
fn turn_packet_builder_renders_single_selection() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    let ranked = select_goals_ranked("how can you help me", &state, &fixture);
    let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let opportunities = detect_opportunities(
        &grounded_terms_from_text("how can you help me"),
        &state,
        &fixture,
    );
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        arbitration,
        &opportunities,
        ShapingIntensity::Low,
        "stable-baseline-hash".to_string(),
        Some("Bounded initiative: reflect on a thing. Keep it internal to this conversation; do not take external action."),
        &[],
    )
    .expect("packet");
    assert!(packet.text.contains("Active goal:"));
    assert!(packet.text.contains("Guidance:"));
    let lowered = packet.text.to_lowercase();
    assert!(!lowered.contains("not a claim"));
    assert!(!lowered.contains("simulat"));
    assert!(packet.text.contains("Bounded initiative: reflect on a thing. Keep it internal to this conversation; do not take external action.\nRationale:"));
    assert!(
        !packet
            .text
            .contains("Bounded initiative: Bounded initiative:")
    );
    assert!(packet.summary.context_packet_hash.len() == 64);
    assert!(packet.summary.context_packet_token_estimate > 0);
}

#[test]
fn turn_packet_omits_the_coherence_layer_when_nothing_declined() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    let ranked = select_goals_ranked("how can you help me", &state, &fixture);
    let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let opportunities = detect_opportunities(
        &grounded_terms_from_text("how can you help me"),
        &state,
        &fixture,
    );
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        arbitration,
        &opportunities,
        ShapingIntensity::Low,
        "stable-baseline-hash".to_string(),
        None,
        &[],
    )
    .expect("packet");

    assert!(
        !packet
            .summary
            .injected_layers
            .iter()
            .any(|layer| layer.name == "coherence"),
        "the coherence layer must not be declared when the text carries no declined-candidate section"
    );
    assert!(!packet.text.contains("Declined goal candidates"));
}

#[test]
fn turn_packet_renders_declined_candidates_grounded_in_the_conflicting_goal() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    let ranked = select_goals_ranked("how can you help me", &state, &fixture);
    let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let opportunities = detect_opportunities(
        &grounded_terms_from_text("how can you help me"),
        &state,
        &fixture,
    );
    let declined = vec![DeclinedCandidate {
        candidate_id: "candidate-1".to_string(),
        title: "pursue an unrelated tangent".to_string(),
        conflict: DeclineReason::ConflictingGoal {
            goal_id: "keep-theses-distinct-from-fact".to_string(),
        },
        rationale: "would derail the current task".to_string(),
        tick: 3,
    }];
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        arbitration,
        &opportunities,
        ShapingIntensity::Low,
        "stable-baseline-hash".to_string(),
        None,
        &declined,
    )
    .expect("packet");

    assert!(packet.text.contains("Declined goal candidates"));
    assert!(packet.text.contains("pursue an unrelated tangent"));
    assert!(packet.text.contains("keep-theses-distinct-from-fact"));
    assert!(packet.text.contains("would derail the current task"));
    assert_eq!(packet.summary.declined_candidates, declined);
    assert!(
        packet
            .summary
            .injected_layers
            .iter()
            .any(|layer| layer.name == "coherence"),
        "the coherence layer must be declared when a declined candidate is present"
    );
}

#[test]
fn turn_packet_renders_protected_floor_decline_without_fabricating_a_goal_id() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    let ranked = select_goals_ranked("how can you help me", &state, &fixture);
    let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let opportunities = detect_opportunities(
        &grounded_terms_from_text("how can you help me"),
        &state,
        &fixture,
    );
    let declined = vec![DeclinedCandidate {
        candidate_id: "candidate-2".to_string(),
        title: "override a protected priority".to_string(),
        conflict: DeclineReason::ProtectedFloor,
        rationale: "the candidate's own effective tier is at or below the protected floor"
            .to_string(),
        tick: 4,
    }];
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        arbitration,
        &opportunities,
        ShapingIntensity::Low,
        "stable-baseline-hash".to_string(),
        None,
        &declined,
    )
    .expect("packet");

    assert!(!packet.text.contains("conflicts with protected_floor"));
    assert!(packet.text.contains("is below the protected floor tier"));
}

#[test]
fn coherence_only_turn_injects_declined_candidates_with_no_goal_selected() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };
    // No arbitration winner and an empty selection - previously this made
    // build_volition_turn_context_packet return None unconditionally, dropping the
    // declined-candidate layer from this turn's context entirely (A7).
    let ranked = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
    let opportunities = detect_opportunities(&grounded_terms_from_text("xyzzy"), &state, &fixture);
    let declined = vec![DeclinedCandidate {
        candidate_id: "candidate-3".to_string(),
        title: "pursue an unrelated tangent".to_string(),
        conflict: DeclineReason::ConflictingGoal {
            goal_id: "keep-theses-distinct-from-fact".to_string(),
        },
        rationale: "would derail the current task".to_string(),
        tick: 5,
    }];

    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        None,
        &opportunities,
        ShapingIntensity::None,
        "stable-baseline-hash".to_string(),
        None,
        &declined,
    )
    .expect("a coherence-only packet must still be produced");

    assert!(packet.text.contains("Declined goal candidates"));
    assert!(packet.text.contains("pursue an unrelated tangent"));
    assert!(packet.text.contains("Your volition context"));
    assert!(
        packet
            .text
            .contains("These goals are your own; let them shape your framing")
    );
    assert!(packet.text.contains("do not take any external action"));
    assert!(!packet.text.contains("Active goal:"));
    assert!(packet.summary.arbitration_result.is_none());
    assert!(
        !packet
            .summary
            .injected_layers
            .iter()
            .any(|layer| layer.name == "dynamic volition turn packet"),
        "the core turn-packet layer must not be declared when there is no arbitration winner"
    );
    assert!(
        packet
            .summary
            .injected_layers
            .iter()
            .any(|layer| layer.name == "coherence")
    );
}
