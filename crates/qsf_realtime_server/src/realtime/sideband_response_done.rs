use std::sync::Arc;
use std::time::{Instant, SystemTime};

use qsf_realtime_protocol::{
    ResponseDoneOutputKind, build_openai_realtime_function_call_output,
    build_openai_realtime_response_create, build_openai_realtime_response_create_with_tool_choice,
    extract_response_text, realtime_event_response_id, realtime_event_response_status,
    realtime_event_text, realtime_response_done_output_kind,
};
use qsf_session::{
    ExchangeModelUse, ExchangeOutput, LiveSessionEvent, ProviderEventKind, ProviderEventRecord,
    ToolExecutionStatus, ToolPermissionDecision, apply_live_session_event,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::diagnostics::DiagnosticRecord;
use crate::realtime::injection::DEFAULT_PCM_RATE_HZ;
use crate::realtime::sideband::{
    SidebandRuntimeState, ensure_authoritative_exchange, hash_request_sequence, send_json,
};
use crate::realtime::sideband_exchange_promotion::promote_completed_trusted_exchanges;
use crate::realtime::sideband_tool_execution::{
    PendingToolExecution, aborted_tool_resolution, execute_realtime_tool_call,
    extract_response_function_call_attempts,
};
use crate::realtime::tools::{
    self, RealtimeToolContext, ToolSessionSnapshot, VolitionStateSnapshot, tool_allow_list,
    tool_permission_decision,
};
use crate::realtime::turn_integrity::TurnPhase;
use crate::state::{AppState, SessionRuntime};

fn response_usage_input_tokens(event: &serde_json::Value) -> u32 {
    response_usage_number(event, &["input_tokens"]).unwrap_or(0) as u32
}

fn response_usage_cached_input_tokens(event: &serde_json::Value) -> u32 {
    response_usage_number(event, &["input_token_details", "cached_tokens"])
        .or_else(|| response_usage_number(event, &["cached_input_tokens"]))
        .unwrap_or(0) as u32
}

fn response_usage_output_tokens(event: &serde_json::Value) -> u32 {
    response_usage_number(event, &["output_tokens"]).unwrap_or(0) as u32
}

fn response_usage_number(event: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = event.get("response")?.get("usage")?;
    for segment in path {
        current = current.get(segment)?;
    }
    current.as_u64()
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_response_done_event(
    state: &AppState,
    qsf_session_id: &str,
    call_id: &str,
    event: &serde_json::Value,
    session: Arc<tokio::sync::Mutex<SessionRuntime>>,
    mut guard: tokio::sync::MutexGuard<'_, SessionRuntime>,
    runtime_state: &mut SidebandRuntimeState,
    outbound_tx: &mpsc::UnboundedSender<Message>,
) -> anyhow::Result<()> {
    let response_id = runtime_state
        .response_id
        .clone()
        .or_else(|| realtime_event_response_id(event).map(str::to_string));
    let response_status = realtime_event_response_status(event).unwrap_or("completed");
    let active_exchange_index = runtime_state.active_exchange_index.or_else(|| {
        guard
            .session_state
            .live
            .active_exchange
            .as_ref()
            .map(|exchange| exchange.index)
    });
    let response_is_stale = response_id
        .as_ref()
        .map(|response_id| runtime_state.stale_response_ids.contains(response_id))
        .unwrap_or(false);
    let exchange_is_stale = match (
        runtime_state.pending_response_exchange,
        active_exchange_index,
    ) {
        (Some(pending_exchange), Some(current_exchange)) => pending_exchange != current_exchange,
        _ => true,
    };
    if response_is_stale || exchange_is_stale {
        guard
            .diagnostics
            .write(&DiagnosticRecord::StaleProviderEvent {
                qsf_session_id: qsf_session_id.to_string(),
                response_id,
                status: Some(response_status.to_string()),
                exchange_index: active_exchange_index,
                at: time::OffsetDateTime::now_utc(),
            })?;
        if response_status == "cancelled" {
            log::info!(
                "ignored stale response.done for session `{qsf_session_id}` with response status `{response_status}`"
            );
        } else {
            log::warn!(
                "ignored stale response.done for session `{qsf_session_id}` with response status `{response_status}`"
            );
        }
        return Ok(());
    }

    let exchange_index = ensure_authoritative_exchange(&mut guard);
    let model_id = guard.config.model.clone();
    let completed_at = SystemTime::now();
    let response_started_at = runtime_state
        .response_started_at
        .unwrap_or_else(Instant::now);
    let response_model_use = ExchangeModelUse {
        provider_name: Some("openai_realtime".to_string()),
        model_id: model_id.clone(),
        latency_ms: response_started_at.elapsed().as_millis() as u64,
        input_tokens: response_usage_input_tokens(event),
        cached_input_tokens: response_usage_cached_input_tokens(event),
        output_tokens: response_usage_output_tokens(event),
        full_request_hash: runtime_state
            .current_request_hash
            .unwrap_or_else(|| hash_request_sequence(&[])),
        message_count: runtime_state.current_message_count.max(1),
    };

    runtime_state.accumulated_latency_ms = runtime_state
        .accumulated_latency_ms
        .saturating_add(response_model_use.latency_ms);
    runtime_state.accumulated_input_tokens = runtime_state
        .accumulated_input_tokens
        .saturating_add(response_model_use.input_tokens);
    runtime_state.accumulated_cached_input_tokens = runtime_state
        .accumulated_cached_input_tokens
        .saturating_add(response_model_use.cached_input_tokens);
    runtime_state.accumulated_output_tokens = runtime_state
        .accumulated_output_tokens
        .saturating_add(response_model_use.output_tokens);

    let output_kind = realtime_response_done_output_kind(event);
    match output_kind {
        ResponseDoneOutputKind::FunctionCallOnly | ResponseDoneOutputKind::Mixed => {
            let function_calls = extract_response_function_call_attempts(event);
            let function_call_names = function_calls
                .iter()
                .map(|call| call.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            log::info!(
                "trusted response.done for session `{qsf_session_id}` exchange `{exchange_index}` classified as {:?} with {} function call attempt(s): [{}]",
                output_kind,
                function_calls.len(),
                function_call_names
            );
            if function_calls.is_empty() {
                log::warn!(
                    "trusted response.done for session `{qsf_session_id}` exchange `{exchange_index}` entered tool handling but no function call attempts were extracted"
                );
            }
            let allow_list = tool_allow_list(&guard.config.tools);
            let registry = guard.tool_registry.clone();
            let snapshot = ToolSessionSnapshot::from_runtime(&guard);
            let voice = guard.config.voice.clone();
            let instructions = guard.config.instructions.clone();
            let event_id = event
                .get("event_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let volition_snapshot = VolitionStateSnapshot {
                state: guard.volition.state.clone(),
                fixture: guard.volition.fixture.clone(),
            };
            let tool_context = RealtimeToolContext {
                state: state.clone(),
                qsf_session_id: qsf_session_id.to_string(),
                snapshot,
                volition: Some(volition_snapshot),
                exchange_index,
                call_id: String::new(),
            };

            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::FunctionCallCompleted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: completed_at,
                    call_id: Some(call_id.to_string()),
                    event_id: Some(
                        event
                            .get("event_id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("response.done")
                            .to_string(),
                    ),
                    item_id: event
                        .get("item_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    previous_item_id: event
                        .get("previous_item_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    response_id: response_id.clone(),
                    text: None,
                    status: Some(response_status.to_string()),
                    audio_marker: event
                        .get("audio_marker")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                }),
            );

            if response_status != "completed" {
                apply_live_session_event(
                    &mut guard.session_state,
                    LiveSessionEvent::ExchangeCompleted {
                        exchange_index,
                        completed_at,
                    },
                );
                guard.non_promotable_exchange_indices.insert(exchange_index);
                promote_completed_trusted_exchanges(state, &mut guard).await?;
                runtime_state.clear_in_flight_response_state();
                runtime_state.active_exchange_index = None;
                runtime_state.pending_response_exchange = None;
                runtime_state.turn_phase = TurnPhase::Idle;
                return Ok(());
            }

            let mut output_messages = Vec::new();
            let mut immediate_resolutions = Vec::new();
            let mut pending_executions = Vec::new();
            let mut force_spoken_response = false;
            for function_call in function_calls {
                let requested_at = SystemTime::now();
                let tool_request = tools::tool_request_record(
                    exchange_index,
                    function_call.call_id.clone(),
                    function_call.name.clone(),
                    function_call.arguments_summary.clone(),
                    requested_at,
                    "openai_realtime",
                );
                apply_live_session_event(
                    &mut guard.session_state,
                    LiveSessionEvent::ToolRequested(tool_request.clone()),
                );

                let decision = if let Some(parse_error) = function_call.parse_error.clone() {
                    ToolPermissionDecision::Denied {
                        reason: format!("function-call arguments were malformed: {parse_error}"),
                    }
                } else if runtime_state.tool_calls_in_turn >= 3 {
                    force_spoken_response = true;
                    ToolPermissionDecision::Denied {
                        reason: "tool loop cap reached (max 3 sequential tool calls per turn)"
                            .to_string(),
                    }
                } else {
                    let metadata = registry.metadata_for(&function_call.name);
                    tool_permission_decision(&function_call.name, &allow_list, metadata.as_ref())
                };

                if matches!(decision, ToolPermissionDecision::Allowed) {
                    let arguments = function_call
                        .arguments
                        .expect("allowed function calls must have parsed arguments");
                    pending_executions.push(PendingToolExecution {
                        name: function_call.name.clone(),
                        call_id: function_call.call_id.clone(),
                        arguments,
                        arguments_summary: function_call.arguments_summary.clone(),
                        requested_at,
                    });
                } else {
                    let reason = match &decision {
                        ToolPermissionDecision::Denied { reason } => reason.clone(),
                        ToolPermissionDecision::Allowed => "tool denied".to_string(),
                    };
                    let execution_record = tools::tool_execution_record(
                        exchange_index,
                        function_call.call_id.clone(),
                        function_call.name.clone(),
                        decision,
                        ToolExecutionStatus::Failed,
                        reason.clone(),
                        Some(reason.clone()),
                        requested_at,
                        Some(SystemTime::now()),
                        Some(response_model_use.clone()),
                        event_id.clone(),
                    );
                    immediate_resolutions.push(execution_record);
                    output_messages.push(build_openai_realtime_function_call_output(
                        &function_call.call_id,
                        &serde_json::json!({
                            "status": "denied",
                            "tool_name": function_call.name,
                            "reason": reason,
                        })
                        .to_string(),
                    ));
                }
                runtime_state.tool_calls_in_turn =
                    runtime_state.tool_calls_in_turn.saturating_add(1);
            }

            drop(guard);

            let mut executed_resolutions = Vec::new();
            for pending in pending_executions {
                let tool_context_for_call = RealtimeToolContext {
                    call_id: pending.call_id.clone(),
                    ..tool_context.clone()
                };
                executed_resolutions.push(execute_realtime_tool_call(
                    &registry,
                    &tool_context_for_call,
                    exchange_index,
                    pending,
                    &response_model_use,
                    event_id.clone(),
                    qsf_session_id,
                ));
            }

            let session_removed = state.session_runtime(qsf_session_id).await.is_none();
            let mut guard = session.lock().await;
            let aborted = guard.degraded || session_removed;
            if aborted {
                guard.non_promotable_exchange_indices.insert(exchange_index);
            }
            for resolution in immediate_resolutions {
                apply_live_session_event(
                    &mut guard.session_state,
                    LiveSessionEvent::ToolResolved(resolution),
                );
            }
            for resolution in executed_resolutions {
                let resolution = if aborted {
                    aborted_tool_resolution(resolution, &response_model_use, event_id.clone())
                } else {
                    output_messages.push(resolution.output_message.clone());
                    resolution
                };
                apply_live_session_event(
                    &mut guard.session_state,
                    LiveSessionEvent::ToolResolved(resolution.record),
                );
            }
            drop(guard);

            if aborted {
                return Ok(());
            }

            for payload in &output_messages {
                send_json(outbound_tx, payload.clone())?;
            }

            let response_create = if force_spoken_response {
                build_openai_realtime_response_create_with_tool_choice(
                    &voice,
                    &instructions,
                    DEFAULT_PCM_RATE_HZ,
                    Some("none"),
                )
            } else {
                build_openai_realtime_response_create(&voice, &instructions, DEFAULT_PCM_RATE_HZ)
            };
            send_json(outbound_tx, response_create.clone())?;
            let mut request_sequence = output_messages.clone();
            request_sequence.push(response_create.clone());
            runtime_state.current_request_hash = Some(hash_request_sequence(&request_sequence));
            runtime_state.current_message_count = request_sequence.len();
            runtime_state.pending_response_exchange = Some(exchange_index);
            runtime_state.turn_phase = TurnPhase::ToolLoop;
            runtime_state.response_id = None;
            runtime_state.response_started_at = None;
            return Ok(());
        }
        ResponseDoneOutputKind::Empty | ResponseDoneOutputKind::Spoken => {}
    }

    let response_text = extract_response_text(event)
        .or_else(|| realtime_event_text(event).map(str::to_string))
        .unwrap_or_default();
    apply_live_session_event(
        &mut guard.session_state,
        LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
            exchange_index,
            event_kind: ProviderEventKind::ResponseCompleted,
            provider_id: "openai_realtime".to_string(),
            received_at: completed_at,
            call_id: Some(call_id.to_string()),
            event_id: Some(
                event
                    .get("event_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("response.done")
                    .to_string(),
            ),
            item_id: event
                .get("item_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            previous_item_id: event
                .get("previous_item_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            response_id: response_id.clone(),
            text: Some(response_text.clone()),
            status: Some(response_status.to_string()),
            audio_marker: event
                .get("audio_marker")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        }),
    );
    apply_live_session_event(
        &mut guard.session_state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: response_id.clone(),
            text: response_text.clone(),
            produced_at: completed_at,
            provider_name: Some("openai_realtime".to_string()),
            target: Some("speech".to_string()),
            audio_marker: event
                .get("audio_marker")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        }),
    );
    apply_live_session_event(
        &mut guard.session_state,
        LiveSessionEvent::ModelRoleCompleted(ExchangeModelUse {
            provider_name: Some("openai_realtime".to_string()),
            model_id,
            latency_ms: runtime_state.accumulated_latency_ms,
            input_tokens: runtime_state.accumulated_input_tokens,
            cached_input_tokens: runtime_state.accumulated_cached_input_tokens,
            output_tokens: runtime_state.accumulated_output_tokens,
            full_request_hash: runtime_state
                .current_request_hash
                .unwrap_or_else(|| hash_request_sequence(&[])),
            message_count: runtime_state.current_message_count.max(1),
        }),
    );
    // Captured before `ExchangeCompleted` is applied below: that event moves `active_exchange`
    // into `completed_exchanges`, so the user input must be read from the still-active exchange
    // now or it is unrecoverable from live session state afterward.
    let completed_turn_user_input = guard
        .session_state
        .live
        .active_exchange
        .as_ref()
        .map(|exchange| exchange.final_user_input().to_string());
    apply_live_session_event(
        &mut guard.session_state,
        LiveSessionEvent::ExchangeCompleted {
            exchange_index,
            completed_at,
        },
    );
    if response_status != "completed" {
        guard.non_promotable_exchange_indices.insert(exchange_index);
        log::warn!(
            "trusted exchange `{exchange_index}` for session `{qsf_session_id}` marked non-promotable because response status was `{response_status}`"
        );
    }
    promote_completed_trusted_exchanges(state, &mut guard).await?;
    let response_dispatched_at = runtime_state.response_create_sent_at;
    runtime_state.clear_in_flight_response_state();
    runtime_state.active_exchange_index = None;
    runtime_state.pending_response_exchange = None;
    runtime_state.turn_phase = TurnPhase::Idle;
    // Captured before drop: gates the live-goal-formation spawn below on the same
    // promotability/degraded facts the promotion pipeline itself just used, so formation never
    // runs on a turn the pipeline distrusts (a cancelled/failed response, a degraded session).
    let session_degraded = guard.degraded;
    let exchange_promotable = !guard
        .non_promotable_exchange_indices
        .contains(&exchange_index);
    let live_goal_formation_eligible = response_status == "completed"
        && exchange_promotable
        && !session_degraded
        && !response_text.trim().is_empty();
    let live_goal_formation_user_input = live_goal_formation_eligible
        .then_some(completed_turn_user_input)
        .flatten();
    drop(guard);

    // Off-hot-path: dispatched after the response, never awaited here, so turn latency is
    // unaffected. See crate::realtime::live_goal_formation. Gated on a completed, promotable,
    // non-degraded, non-empty assistant turn - a barge-in mid-answer or a degraded session must
    // not form a durable goal (or a permanently injected declined record) from a half-spoken or
    // untrusted turn.
    if let Some(user_input) = live_goal_formation_user_input {
        let turn_transcript = qsf_models::format_exchange_transcript(&user_input, &response_text);
        crate::realtime::live_goal_formation::spawn_live_goal_formation(
            session,
            qsf_session_id.to_string(),
            exchange_index,
            turn_transcript,
            response_dispatched_at,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_usage_extractors_tolerate_missing_fields() {
        let event = serde_json::json!({
            "response": {
                "usage": {
                    "input_tokens": 3,
                    "input_token_details": {
                        "cached_tokens": 1
                    },
                    "output_tokens": 4
                }
            }
        });

        assert_eq!(response_usage_input_tokens(&event), 3);
        assert_eq!(response_usage_cached_input_tokens(&event), 1);
        assert_eq!(response_usage_output_tokens(&event), 4);
    }
}
