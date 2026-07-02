use std::mem;
use std::sync::Arc;

use qsf_context::ContextBudget;
use qsf_memory::RetrievalStrategy;
use qsf_realtime_protocol::build_openai_realtime_response_create;
use qsf_session::{ContentHash, LiveSessionEvent, apply_live_session_event};
use qsf_volition::{
    GroundingRef, OpportunitySignal, OpportunitySignalKind, ReceptivenessHint, VolitionEvent,
    arbitrate_with_mode, build_state_inspection, choose_shaping_intensity, detect_opportunities,
    execute_initiative, grounded_terms_from_text, select_goals_ranked,
};
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::diagnostics::DiagnosticRecord;
use crate::realtime::injection::{
    DEFAULT_PCM_RATE_HZ, MemoryInjectionRequest, assemble_memory_context,
    build_memory_injection_packet,
};
use crate::realtime::memory_store::retrieve_session_memories;
use crate::realtime::sideband::{
    DEFAULT_INJECTION_FRAGMENT_LIMIT, DEFAULT_INJECTION_TOKEN_LIMIT, SidebandRuntimeState,
    hash_request_sequence, record_latency_observation_if_ready, send_json,
};
use crate::realtime::tools::VolitionStateSnapshot;
use crate::realtime::turn_context::{TurnContextCapture, build_turn_context_capture};
use crate::realtime::turn_integrity::TurnPhase;
use crate::realtime::volition_initiative::{
    build_realtime_bounded_initiative_trace, render_initiative_line,
};
use crate::realtime::volition_injection::{
    build_volition_context_injection_trace, build_volition_turn_context_packet,
};
use crate::realtime::volition_inspection_capture::{
    VolitionInspectionCapture, build_volition_inspection_capture,
    build_volition_turn_decision_summary,
};
use crate::state::{AppState, SessionRuntime};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn inject_trusted_turn_context_and_response(
    state: &AppState,
    session: Arc<tokio::sync::Mutex<SessionRuntime>>,
    qsf_session_id: &str,
    transcript: &str,
    config: &crate::state::BrowserSessionConfig,
    runtime_state: &mut SidebandRuntimeState,
    outbound_tx: &mpsc::UnboundedSender<Message>,
    leading_conversation_item_create: Option<serde_json::Value>,
    exchange_index: usize,
    volition_snapshot: &VolitionStateSnapshot,
    events_applied: Vec<qsf_volition::VolitionEvent>,
    volition_tick_before: u64,
    input_transcript_ref: String,
) -> anyhow::Result<()> {
    let pending_context_retrieval_hints =
        mem::take(&mut runtime_state.pending_context_retrieval_hints);
    let hint_consumed_by_next_memory_injection = !pending_context_retrieval_hints.is_empty();
    let retrieval_query = if hint_consumed_by_next_memory_injection {
        format!("{transcript} {}", pending_context_retrieval_hints.join(" "))
    } else {
        transcript.to_string()
    };

    let retrieved_memories = match retrieve_session_memories(
        state,
        qsf_session_id,
        &retrieval_query,
        RetrievalStrategy::AssociationWeighted,
        DEFAULT_INJECTION_FRAGMENT_LIMIT,
    ) {
        Ok(result) => result.selected,
        Err(error) => {
            log::warn!(
                "memory retrieval failed for trusted turn in session `{qsf_session_id}`: {error}"
            );
            Vec::new()
        }
    };

    let tone = format!(
        "voice={}, reasoning_effort={}",
        config.voice, config.reasoning_effort
    );
    let request = MemoryInjectionRequest {
        model: &config.model,
        voice: &config.voice,
        base_instructions: &config.instructions,
        output_modalities: &config.output_modalities,
        session_identity: qsf_session_id,
        tone: &tone,
        user_transcript: transcript,
        retrieved_memories: &retrieved_memories,
        budget: ContextBudget::new(
            DEFAULT_INJECTION_FRAGMENT_LIMIT,
            DEFAULT_INJECTION_TOKEN_LIMIT,
        ),
        pcm_rate_hz: DEFAULT_PCM_RATE_HZ,
        input_transcription_model: config.input_transcription_model.as_deref(),
    };
    let memory_packet = build_memory_injection_packet(&request);
    let context_assembly = memory_packet
        .as_ref()
        .map(|packet| packet.context_assembly.clone())
        .unwrap_or_else(|| assemble_memory_context(&request));
    let retrieved_memory_block = memory_packet
        .as_ref()
        .map(|packet| packet.memory_block.clone())
        .unwrap_or_default();
    let memory_session_update = memory_packet
        .as_ref()
        .map(|packet| packet.session_update.clone());
    let memory_conversation_item_create = memory_packet
        .as_ref()
        .map(|packet| packet.conversation_item_create.clone());

    let mut guard = session.lock().await;
    let diagnostics = guard.diagnostics.clone();
    let turn_context_tx = guard.turn_context_sender();
    let volition_inspection_tx = guard.volition_inspection_sender();
    apply_live_session_event(
        &mut guard.session_state,
        LiveSessionEvent::MemoryContextRecorded {
            exchange_index,
            context_assembly,
            retrieved_memory_block,
            recalled_items: vec![],
            live_capture: None,
        },
    );
    runtime_state.memory_injected_at = Some(OffsetDateTime::now_utc());
    let final_transcript_received_at = runtime_state.final_transcript_received_at;
    let memory_injected_at = runtime_state.memory_injected_at;
    record_latency_observation_if_ready(
        runtime_state,
        &diagnostics,
        qsf_session_id,
        "final_transcript_received_to_memory_injected",
        final_transcript_received_at,
        memory_injected_at,
    )?;
    drop(guard);

    let mut turn_request_values = Vec::new();
    if let Some(conversation_item_create) = leading_conversation_item_create {
        turn_request_values.push(conversation_item_create.clone());
        send_json(outbound_tx, conversation_item_create)?;
    }
    if let Some(session_update) = memory_session_update {
        turn_request_values.push(session_update.clone());
        send_json(outbound_tx, session_update)?;
    }
    if let Some(conversation_item_create) = memory_conversation_item_create {
        turn_request_values.push(conversation_item_create.clone());
        send_json(outbound_tx, conversation_item_create)?;
    }

    let grounded_terms = grounded_terms_from_text(transcript);
    let ranked = select_goals_ranked(
        transcript,
        &volition_snapshot.state,
        &volition_snapshot.fixture,
    );
    let opportunities = detect_opportunities(
        &grounded_terms,
        &volition_snapshot.state,
        &volition_snapshot.fixture,
    );
    let arbitration = arbitrate_with_mode(
        ranked.selected.clone(),
        &volition_snapshot.fixture,
        volition_snapshot.state.mode,
    );
    let intensity = arbitration
        .as_ref()
        .map(|arbitration| {
            choose_shaping_intensity(arbitration, &opportunities, ReceptivenessHint::Neutral)
        })
        .unwrap_or(qsf_volition::ShapingIntensity::None);
    let mut initiative_line = None;
    let mut initiative_state_snapshot_before = None;
    let mut initiative_state_snapshot_after = None;
    let mut initiative_output = None;
    let mut initiative_context_retrieval_hint_terms = None;
    let mut surfaced = false;
    let mut suppression_reason = None;
    let mut rendered_line_present = false;

    if let Some(arbitration) = arbitration.as_ref() {
        let output = execute_initiative(&arbitration.winner.initiative, &arbitration.winner.goal);
        let context_retrieval_hint_terms = match &output {
            qsf_volition::InitiativeOutput::ContextRetrievalRequested { query_terms } => {
                Some(query_terms.clone())
            }
            _ => None,
        };
        if let Some(query_terms) = context_retrieval_hint_terms.as_ref() {
            runtime_state
                .pending_context_retrieval_hints
                .extend(query_terms.iter().cloned());
        }

        let protected_winner =
            arbitration.winner_bias.effective_tier <= qsf_volition::PROTECTED_TIER_FLOOR;
        let genuine_opportunity =
            has_genuine_opportunity_signal(&opportunities, &arbitration.winner.goal.id);
        let repeated_goal = runtime_state.previous_turn_surfaced_goal_id.as_deref()
            == Some(arbitration.winner.goal.id.as_str());
        let renderable_line = render_initiative_line(&output, intensity);
        let suppression_reason_local = if matches!(intensity, qsf_volition::ShapingIntensity::None)
        {
            Some(qsf_volition::VolitionSuppressionReason::Intensity)
        } else if protected_winner && !genuine_opportunity {
            Some(qsf_volition::VolitionSuppressionReason::ProtectedNoOpportunity)
        } else if repeated_goal {
            Some(qsf_volition::VolitionSuppressionReason::AntiNagRepeat)
        } else if renderable_line.is_none() {
            Some(qsf_volition::VolitionSuppressionReason::NonRenderableOutput)
        } else {
            None
        };
        let surfaced_line = suppression_reason_local
            .is_none()
            .then_some(renderable_line)
            .flatten();
        surfaced = surfaced_line.is_some();
        rendered_line_present = surfaced;
        suppression_reason = suppression_reason_local;

        let mut guard = session.lock().await;
        let state_snapshot_before =
            build_state_inspection(&guard.volition.state, &guard.volition.fixture);
        let initiative_event = VolitionEvent::InitiativeExecuted {
            goal_id: arbitration.winner.goal.id.clone(),
            effect: arbitration.winner.initiative.effect,
            output: output.clone(),
            rationale: arbitration.winner.initiative.rationale.clone(),
            tick: guard.volition.state.tick,
        };
        guard.volition.apply_events(vec![initiative_event]);
        let state_snapshot_after =
            build_state_inspection(&guard.volition.state, &guard.volition.fixture);
        drop(guard);

        runtime_state.previous_turn_surfaced_goal_id = surfaced_line
            .as_ref()
            .map(|_| arbitration.winner.goal.id.clone());
        initiative_line = surfaced_line;
        initiative_state_snapshot_before = Some(state_snapshot_before);
        initiative_state_snapshot_after = Some(state_snapshot_after);
        initiative_output = Some(output);
        initiative_context_retrieval_hint_terms = context_retrieval_hint_terms;
    } else {
        runtime_state.previous_turn_surfaced_goal_id = None;
    }

    // Reuse the caller's snapshot rather than taking a third `session.lock()` on this path —
    // declined_candidates is reducer-derived state living on `volition_snapshot.state`, and
    // nothing before this point in the turn mutates it.
    let volition_packet = build_volition_turn_context_packet(
        volition_snapshot,
        &ranked,
        arbitration.clone(),
        &opportunities,
        intensity,
        crate::realtime::volition_injection::stable_baseline_hash(&config.instructions),
        initiative_line.as_deref(),
        &volition_snapshot.state.declined_candidates,
    );
    debug_assert!(
        initiative_output.is_none() || volition_packet.is_some(),
        "arbitration-backed initiative mutation must have a volition packet so the trace is emitted"
    );

    if let Some(turn_packet) = &volition_packet {
        turn_request_values.push(turn_packet.conversation_item_create.clone());
        send_json(outbound_tx, turn_packet.conversation_item_create.clone())?;
        runtime_state.volition_context_injected_at = Some(OffsetDateTime::now_utc());
        let volition_context_injected_at = runtime_state.volition_context_injected_at;
        record_latency_observation_if_ready(
            runtime_state,
            &diagnostics,
            qsf_session_id,
            "final_transcript_received_to_volition_context_injected",
            runtime_state.final_transcript_received_at,
            volition_context_injected_at,
        )?;
        let response_create = build_openai_realtime_response_create(
            &config.voice,
            &config.instructions,
            DEFAULT_PCM_RATE_HZ,
        );
        turn_request_values.push(response_create.clone());
        let request_hash = hash_request_sequence(&turn_request_values);
        let initiative_trace = initiative_output.as_ref().map(|output| {
            let winner = &arbitration
                .as_ref()
                .expect("initiative trace requires arbitration winner")
                .winner;
            build_realtime_bounded_initiative_trace(
                qsf_session_id,
                exchange_index,
                winner,
                output.clone(),
                surfaced,
                suppression_reason,
                rendered_line_present,
                initiative_state_snapshot_before
                    .clone()
                    .expect("initiative trace requires state snapshot before"),
                initiative_state_snapshot_after
                    .clone()
                    .expect("initiative trace requires state snapshot after"),
                initiative_context_retrieval_hint_terms.clone(),
                hint_consumed_by_next_memory_injection,
                &request_hash.to_string(),
            )
        });
        let mut trace = build_volition_context_injection_trace(
            qsf_session_id,
            exchange_index,
            &input_transcript_ref,
            volition_tick_before,
            events_applied,
            turn_packet,
            &request_hash.to_string(),
        );
        if memory_packet.is_some() {
            trace.injected_layers.insert(
                1,
                crate::realtime::volition_injection::VolitionInjectionLayer {
                    name: "memory context".to_string(),
                    carrier: "conversation.item.create".to_string(),
                    injection_point: "after retrieval and before volition turn packet".to_string(),
                },
            );
        }
        diagnostics.write(&DiagnosticRecord::VolitionContextInjected {
            qsf_session_id: qsf_session_id.to_string(),
            exchange_index,
            recorded_at: OffsetDateTime::now_utc(),
            trace,
        })?;
        if let Some(initiative_trace) = initiative_trace {
            diagnostics.write(&DiagnosticRecord::RealtimeBoundedInitiative {
                qsf_session_id: qsf_session_id.to_string(),
                exchange_index,
                recorded_at: OffsetDateTime::now_utc(),
                trace: initiative_trace,
            })?;
        }
        let volition_inspection_capture = {
            let guard = session.lock().await;
            let inspection = build_state_inspection(&guard.volition.state, &guard.volition.fixture);
            build_volition_inspection_capture(
                qsf_session_id.to_string(),
                exchange_index,
                OffsetDateTime::now_utc(),
                request_hash.to_string(),
                inspection,
                arbitration.as_ref().and_then(|arbitration| {
                    initiative_output.as_ref().map(|output| {
                        build_volition_turn_decision_summary(
                            &ranked,
                            arbitration,
                            output,
                            surfaced,
                            suppression_reason,
                            rendered_line_present,
                            intensity,
                        )
                    })
                }),
            )
        };
        return send_response_create_and_capture(
            outbound_tx,
            response_create,
            turn_request_values,
            request_hash,
            exchange_index,
            runtime_state,
            &diagnostics,
            qsf_session_id,
            memory_injected_at,
            &turn_context_tx,
            &volition_inspection_tx,
            volition_inspection_capture,
        );
    }

    let response_create = build_openai_realtime_response_create(
        &config.voice,
        &config.instructions,
        DEFAULT_PCM_RATE_HZ,
    );
    turn_request_values.push(response_create.clone());
    let request_hash = hash_request_sequence(&turn_request_values);
    let volition_inspection_capture = {
        let guard = session.lock().await;
        let inspection = build_state_inspection(&guard.volition.state, &guard.volition.fixture);
        build_volition_inspection_capture(
            qsf_session_id.to_string(),
            exchange_index,
            OffsetDateTime::now_utc(),
            request_hash.to_string(),
            inspection,
            None,
        )
    };
    send_response_create_and_capture(
        outbound_tx,
        response_create,
        turn_request_values,
        request_hash,
        exchange_index,
        runtime_state,
        &diagnostics,
        qsf_session_id,
        memory_injected_at,
        &turn_context_tx,
        &volition_inspection_tx,
        volition_inspection_capture,
    )
}

/// Send `response.create` and, on success, publish a [`TurnContextCapture`] to the
/// session's turn-context and volition-inspection watch channels.
///
/// Both branches of [`inject_trusted_turn_context_and_response`] call this
/// helper so the publish is guaranteed to be consistent with the hash for every
/// trusted turn.
///
/// The `request_hash` and volition inspection capture must be pre-computed by
/// the caller. This function never calls `hash_request_sequence` internally.
///
/// The publish happens **only** after `send_json` returns without error.  If
/// `send_json` fails the `?` propagates and the watch channel is not updated.
#[allow(clippy::too_many_arguments)]
fn send_response_create_and_capture(
    outbound_tx: &mpsc::UnboundedSender<Message>,
    response_create: serde_json::Value,
    turn_request_values: Vec<serde_json::Value>,
    request_hash: ContentHash,
    exchange_index: usize,
    runtime_state: &mut SidebandRuntimeState,
    diagnostics: &crate::diagnostics::DiagnosticWriter,
    qsf_session_id: &str,
    memory_injected_at: Option<OffsetDateTime>,
    turn_context_tx: &watch::Sender<Option<TurnContextCapture>>,
    volition_inspection_tx: &watch::Sender<Option<VolitionInspectionCapture>>,
    volition_inspection_capture: VolitionInspectionCapture,
) -> anyhow::Result<()> {
    send_json(outbound_tx, response_create)?;
    runtime_state.response_create_sent_at = Some(OffsetDateTime::now_utc());
    let response_create_sent_at = runtime_state.response_create_sent_at;
    record_latency_observation_if_ready(
        runtime_state,
        diagnostics,
        qsf_session_id,
        "memory_injected_to_response_create_sent",
        memory_injected_at,
        response_create_sent_at,
    )?;
    let message_count = turn_request_values.len();
    let capture = build_turn_context_capture(
        qsf_session_id.to_string(),
        exchange_index,
        request_hash.to_string(),
        turn_request_values,
    );
    turn_context_tx.send_replace(Some(capture));
    volition_inspection_tx.send_replace(Some(volition_inspection_capture));
    runtime_state.current_request_hash = Some(request_hash);
    runtime_state.current_message_count = message_count;
    runtime_state.pending_response_exchange = Some(exchange_index);
    runtime_state.turn_phase = TurnPhase::AwaitingResponse;
    Ok(())
}

fn has_genuine_opportunity_signal(
    opportunities: &[OpportunitySignal],
    winner_goal_id: &str,
) -> bool {
    opportunities.iter().any(|signal| match &signal.kind {
        OpportunitySignalKind::ExpressedUncertainty
        | OpportunitySignalKind::IntroducedContradiction => true,
        OpportunitySignalKind::OpenGoalTopicMatch => match &signal.grounding_ref {
            GroundingRef::GoalId { goal_id } => goal_id != winner_goal_id,
            GroundingRef::InputTerm { .. } => false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_volition_inspection_capture()
    -> crate::realtime::volition_inspection_capture::VolitionInspectionCapture {
        use crate::realtime::volition_inspection_capture::build_volition_inspection_capture;
        use qsf_volition::{VolitionState, build_state_inspection, realtime_seed_fixture};

        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        build_volition_inspection_capture(
            "test-session".to_string(),
            0,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            build_state_inspection(&state, &fixture),
            None,
        )
    }

    /// Verifies the fidelity contract: the `request_hash` stored in the published
    /// `TurnContextCapture` equals `hash_request_sequence(&turn_request_values)` for
    /// the same turn values, and the `messages` field is the verbatim
    /// `turn_request_values`.
    #[test]
    fn captured_request_hash_matches_hash_of_messages() {
        use crate::realtime::turn_context::TurnContextCapture;
        use tokio::sync::watch;

        let turn_request_values = vec![
            serde_json::json!({"type": "conversation.item.create", "item": {"role": "user"}}),
            serde_json::json!({"type": "response.create"}),
        ];
        let expected_hash = hash_request_sequence(&turn_request_values);
        let expected_hash_string = expected_hash.to_string();
        let expected_messages = turn_request_values.clone();
        let response_create = turn_request_values.last().expect("last item").clone();

        let (outbound_tx, _outbound_rx) = mpsc::unbounded_channel::<Message>();
        let (turn_context_tx, turn_context_rx) = watch::channel::<Option<TurnContextCapture>>(None);
        let (volition_inspection_tx, volition_inspection_rx) = watch::channel(None);

        let memory_injected_at = None;
        let mut runtime_state = SidebandRuntimeState::default();

        let tempdir = tempfile::TempDir::new().expect("tempdir");
        let diagnostics =
            crate::diagnostics::DiagnosticWriter::create(tempdir.path().join("fidelity.jsonl"))
                .expect("diagnostics");

        let result = send_response_create_and_capture(
            &outbound_tx,
            response_create,
            turn_request_values,
            expected_hash,
            0,
            &mut runtime_state,
            &diagnostics,
            "test-session",
            memory_injected_at,
            &turn_context_tx,
            &volition_inspection_tx,
            dummy_volition_inspection_capture(),
        );

        assert!(
            result.is_ok(),
            "send_response_create_and_capture must succeed when the outbound channel is open"
        );
        let capture = turn_context_rx
            .borrow()
            .clone()
            .expect("turn_context must be published after successful send");
        assert_eq!(
            capture.request_hash, expected_hash_string,
            "captured request_hash must equal hash_request_sequence of the captured messages"
        );
        assert_eq!(
            capture.messages, expected_messages,
            "captured messages must be the verbatim turn_request_values in send order"
        );
        assert!(volition_inspection_rx.borrow().is_some());
    }

    /// Verifies the failure-path contract: if `outbound_tx` is closed before
    /// `send_json` is called, `send_response_create_and_capture` returns an
    /// error and the turn-context watch channel is NOT updated.
    #[test]
    fn failed_send_does_not_publish_turn_context_capture() {
        use crate::realtime::turn_context::TurnContextCapture;
        use time::OffsetDateTime;
        use tokio::sync::watch;

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<Message>();
        // Drop the receiver so that every subsequent send will fail immediately.
        drop(outbound_rx);

        let (turn_context_tx, turn_context_rx) = watch::channel::<Option<TurnContextCapture>>(None);
        let (volition_inspection_tx, volition_inspection_rx) = watch::channel(None);

        let memory_injected_at = Some(OffsetDateTime::now_utc());
        let mut runtime_state = SidebandRuntimeState {
            memory_injected_at,
            ..SidebandRuntimeState::default()
        };

        let tempdir = tempfile::TempDir::new().expect("tempdir");
        let diagnostics =
            crate::diagnostics::DiagnosticWriter::create(tempdir.path().join("test.jsonl"))
                .expect("diagnostics");

        let turn_request_values = vec![serde_json::json!({"type": "response.create"})];
        let request_hash = hash_request_sequence(&turn_request_values);
        let response_create = serde_json::json!({"type": "response.create"});

        let result = send_response_create_and_capture(
            &outbound_tx,
            response_create,
            turn_request_values,
            request_hash,
            0,
            &mut runtime_state,
            &diagnostics,
            "test-session",
            memory_injected_at,
            &turn_context_tx,
            &volition_inspection_tx,
            dummy_volition_inspection_capture(),
        );

        assert!(
            result.is_err(),
            "send_response_create_and_capture must fail when the outbound channel is closed"
        );
        assert!(
            turn_context_rx.borrow().is_none(),
            "turn_context must not be published when send_json fails"
        );
        assert!(
            volition_inspection_rx.borrow().is_none(),
            "volition inspection must not be published when send_json fails"
        );
    }
}
