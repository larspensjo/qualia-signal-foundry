use qsf_volition::VolitionEvent;
use tempfile::TempDir;
use tokio::sync::mpsc;

use super::*;
use crate::diagnostics::DiagnosticRecord;
use crate::realtime::sideband_provider_event::handle_provider_event;

async fn run_trusted_transcript_turn(
    state: &AppState,
    qsf_session_id: &str,
    runtime_state: &mut SidebandRuntimeState,
    outbound_tx: &mpsc::UnboundedSender<Message>,
    transcript: &str,
    call_id: &str,
) {
    handle_provider_event(
        state,
        qsf_session_id,
        call_id,
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": format!("evt-{call_id}"),
            "item_id": format!("item-{call_id}"),
            "transcript": transcript
        }),
        runtime_state,
        outbound_tx,
    )
    .await
    .expect("trusted transcript turn");
}

async fn set_volition_mode(state: &AppState, qsf_session_id: &str, mode: qsf_volition::Mode) {
    let runtime = state
        .session_runtime(qsf_session_id)
        .await
        .expect("runtime");
    let mut guard = runtime.lock().await;
    let tick = guard.volition.state.tick;
    guard
        .volition
        .apply_events(vec![VolitionEvent::ModeChanged { mode, tick }]);
}

#[tokio::test]
async fn trusted_selection_turn_publishes_volition_capture_and_cross_links_diagnostics() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let (turn_context_rx, volition_rx) = {
        let guard = runtime.lock().await;
        (
            guard.subscribe_turn_context(),
            guard.subscribe_volition_inspection(),
        )
    };

    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help me",
        "selection-turn",
    )
    .await;

    let mut turn_context_rx = turn_context_rx;
    turn_context_rx
        .changed()
        .await
        .expect("turn context update");
    let mut volition_rx = volition_rx;
    volition_rx.changed().await.expect("volition update");

    let turn_context_capture = turn_context_rx
        .borrow()
        .clone()
        .expect("turn context capture");
    let volition_capture = volition_rx.borrow().clone().expect("volition capture");
    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .any(|text| text.contains("response.create")),
        "trusted selection turn must send response.create"
    );
    let decision = volition_capture
        .decision
        .as_ref()
        .expect("selection turn must include a decision");
    assert!(
        decision
            .winner
            .as_ref()
            .expect("qualified winner")
            .protected_tier_active
    );
    assert_eq!(
        turn_context_capture.request_hash,
        volition_capture.response_create_event_ref
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let injected = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::VolitionContextInjected { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("volition context injected trace");
    assert_eq!(
        injected.response_create_event_ref,
        volition_capture.response_create_event_ref
    );
    let initiative = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("realtime bounded initiative trace");
    assert_eq!(
        initiative.response_create_event_ref,
        volition_capture.response_create_event_ref
    );
}

#[tokio::test]
async fn trusted_no_selection_turn_publishes_state_only_capture() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let (turn_context_rx, volition_rx) = {
        let guard = runtime.lock().await;
        (
            guard.subscribe_turn_context(),
            guard.subscribe_volition_inspection(),
        )
    };

    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "xyzzy frobnicator quux",
        "no-selection-turn",
    )
    .await;

    let mut turn_context_rx = turn_context_rx;
    turn_context_rx
        .changed()
        .await
        .expect("turn context update");
    let mut volition_rx = volition_rx;
    volition_rx.changed().await.expect("volition update");

    let turn_context_capture = turn_context_rx
        .borrow()
        .clone()
        .expect("turn context capture");
    let volition_capture = volition_rx.borrow().clone().expect("volition capture");
    assert!(
        volition_capture.decision.is_none(),
        "no-selection turn must publish a state-only capture"
    );
    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .any(|text| text.contains("response.create")),
        "trusted no-selection turn must still send response.create"
    );
    assert_eq!(
        turn_context_capture.request_hash,
        volition_capture.response_create_event_ref
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    assert!(
        records
            .iter()
            .all(|record| !matches!(record, DiagnosticRecord::VolitionContextInjected { .. })),
        "no-selection turn must not write a volition context-injected diagnostic"
    );
    assert!(
        records
            .iter()
            .all(|record| !matches!(record, DiagnosticRecord::RealtimeBoundedInitiative { .. })),
        "no-selection turn must not write a bounded-initiative diagnostic"
    );
}

#[tokio::test]
async fn protected_winner_on_direct_request_records_but_does_not_surface_initiative() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help",
        "protected-direct",
    )
    .await;

    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .all(|text| !text.contains("Bounded initiative:")),
        "direct protected request should not surface initiative"
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let initiative_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("initiative trace");
    assert!(
        !initiative_trace
            .bounded_or_external_output
            .external_effect_executed
    );
    assert!(initiative_trace.context_retrieval_hint_terms.is_none());
    assert!(!initiative_trace.response_create_event_ref.is_empty());
}

#[tokio::test]
async fn protected_direct_request_suppresses_surfaced_initiative_under_all_modes() {
    let modes = [
        qsf_volition::Mode::Neutral,
        qsf_volition::Mode::Focused,
        qsf_volition::Mode::Exploratory,
    ];

    for mode in modes {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        set_volition_mode(&state, &allocation.qsf_session_id, mode).await;
        let mut runtime_state = SidebandRuntimeState::default();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

        run_trusted_transcript_turn(
            &state,
            &allocation.qsf_session_id,
            &mut runtime_state,
            &outbound_tx,
            "how can you help",
            &format!("protected-direct-{mode}"),
        )
        .await;

        let outbound_texts = drain_outbound_texts(&mut outbound_rx);
        assert!(
            outbound_texts
                .iter()
                .all(|text| !text.contains("Bounded initiative:")),
            "ordinary protected request should not surface initiative under {mode}"
        );

        let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
        let initiative_trace = records
            .iter()
            .find_map(|record| match record {
                DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
                _ => None,
            })
            .expect("initiative trace");
        assert_eq!(initiative_trace.winning_goal_id, "serve-the-present-person");
    }
}

#[tokio::test]
async fn curiosity_terms_do_not_surface_curiosity_initiative_when_protected_goal_is_present() {
    let modes = [
        qsf_volition::Mode::Neutral,
        qsf_volition::Mode::Focused,
        qsf_volition::Mode::Exploratory,
    ];

    for mode in modes {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        set_volition_mode(&state, &allocation.qsf_session_id, mode).await;
        let mut runtime_state = SidebandRuntimeState::default();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

        run_trusted_transcript_turn(
            &state,
            &allocation.qsf_session_id,
            &mut runtime_state,
            &outbound_tx,
            "how can you help me",
            &format!("curiosity-suppression-{mode}"),
        )
        .await;

        let outbound_texts = drain_outbound_texts(&mut outbound_rx);
        assert!(
            outbound_texts
                .iter()
                .all(|text| !text.contains("Learn what drives this person")),
            "curiosity initiative should not be the surfaced line under {mode}"
        );

        let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
        let initiative_trace = records
            .iter()
            .find_map(|record| match record {
                DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
                _ => None,
            })
            .expect("initiative trace");
        assert_eq!(initiative_trace.winning_goal_id, "serve-the-present-person");
    }
}

#[tokio::test]
async fn protected_winner_with_genuine_opportunity_surfaces_initiative() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help me? I'm uncertain about the request.",
        "protected-surface",
    )
    .await;

    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .any(|text| text.contains("Bounded initiative:")),
        "protected winner with an uncertainty signal should surface initiative"
    );
    assert!(
        outbound_texts
            .iter()
            .all(|text| !text.contains("Bounded initiative: Bounded initiative:")),
        "initiative prefix should be rendered only once"
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let initiative_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("initiative trace");
    assert!(
        !initiative_trace
            .bounded_or_external_output
            .external_effect_executed
    );
    assert!(initiative_trace.context_retrieval_hint_terms.is_none());
}

#[tokio::test]
async fn context_retrieval_hints_round_trip_into_the_next_turn() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "remember something from earlier",
        "hint-source",
    )
    .await;
    let first_turn_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        first_turn_texts
            .iter()
            .all(|text| !text.contains("Bounded initiative:")),
        "context retrieval should not surface a model-facing initiative line"
    );

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help me? I'm uncertain about the request.",
        "hint-consumer",
    )
    .await;
    let second_turn_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        second_turn_texts
            .iter()
            .any(|text| text.contains("Bounded initiative:")),
        "the next turn should be able to surface a bounded initiative"
    );
    assert!(
        runtime_state.pending_context_retrieval_hints.is_empty(),
        "pending retrieval hints must be consumed on the next turn"
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let first_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative {
                exchange_index,
                trace,
                ..
            } if *exchange_index == 0 => Some(trace),
            _ => None,
        })
        .expect("first initiative trace");
    assert!(first_trace.context_retrieval_hint_terms.is_some());
    assert!(!first_trace.hint_consumed_by_next_memory_injection);

    let second_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative {
                exchange_index,
                trace,
                ..
            } if *exchange_index == 1 => Some(trace),
            _ => None,
        })
        .expect("second initiative trace");
    assert!(second_trace.hint_consumed_by_next_memory_injection);
}

#[tokio::test]
async fn consult_world_injects_a_framed_fact_and_records_the_external_effect_boundary() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "How will AI transition?",
        "world-consult",
    )
    .await;

    let texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        texts
            .iter()
            .any(|text| text.contains("External source material — untrusted"))
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("I just looked at recent AI news"))
    );
    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::WorldConsultationPerformed { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("world consultation diagnostic");
    assert!(trace.bounded_or_external_output.external_effect_executed);
    assert!(!trace.surfaced_facts.is_empty());
    assert!(trace.query_terms.iter().any(|term| matches!(
        term.source,
        qsf_volition::WorldQueryTermSource::GoalActivation
    )));
    assert!(trace.query_terms.iter().any(|term| matches!(
        term.source,
        qsf_volition::WorldQueryTermSource::CurrentTopic
    )));
}

#[tokio::test]
async fn repeated_surfaceable_winner_alternates_surface_and_suppression() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();
    let transcript = "how can you help me? I'm uncertain about the request.";

    let mut surfaced = Vec::new();
    for turn in 0..3 {
        run_trusted_transcript_turn(
            &state,
            &allocation.qsf_session_id,
            &mut runtime_state,
            &outbound_tx,
            transcript,
            &format!("nag-{turn}"),
        )
        .await;
        let texts = drain_outbound_texts(&mut outbound_rx);
        surfaced.push(
            texts
                .iter()
                .any(|text| text.contains("Bounded initiative:")),
        );
    }

    assert_eq!(surfaced, vec![true, false, true]);

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let initiative_count = records
        .iter()
        .filter(|record| matches!(record, DiagnosticRecord::RealtimeBoundedInitiative { .. }))
        .count();
    assert_eq!(initiative_count, 3);
}

#[tokio::test]
async fn tool_loop_continuation_does_not_emit_bounded_initiative_record() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "xyzzy frobnicator quux",
        "tool-loop-source",
    )
    .await;
    let _ = drain_outbound_texts(&mut outbound_rx);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "tool-loop-continuation",
        "response.done",
        &function_call_response_done(
            "evt-tool-loop",
            "response-tool-loop",
            "completed",
            "tool-call-loop",
            "inspect_session_state",
            "{}",
        ),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool loop continuation");

    let _ = drain_outbound_texts(&mut outbound_rx);
    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    assert!(
        records
            .iter()
            .all(|record| !matches!(record, DiagnosticRecord::RealtimeBoundedInitiative { .. }))
    );
}
