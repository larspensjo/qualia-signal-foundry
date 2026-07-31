use time::OffsetDateTime;

use crate::diagnostics::DiagnosticRecord;
use crate::state::{AppState, SessionRuntime};

use qsf_session::LiveSessionEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StopResult {
    pub qsf_session_id: String,
    pub completed_exchanges: usize,
}

/// Stop a session through the one shared lifecycle path used by HTTP and scripted runs.
pub(crate) async fn stop_session(
    state: &AppState,
    qsf_session_id: String,
) -> anyhow::Result<StopResult> {
    let session = state
        .remove_session(&qsf_session_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown qsf_session_id `{}`", qsf_session_id))?;
    let (sideband, stop_session_id, completed, lifecycle_error) = {
        let mut guard = session.lock().await;
        let before = guard.relay_state.completed_exchanges.len();
        finalize_open_exchange(&mut guard);
        let mut lifecycle_error = persist_completed_diagnostic_exchanges(&mut guard).err();
        let completed = guard
            .relay_state
            .completed_exchanges
            .len()
            .saturating_sub(before);
        let stop_session_id = guard.qsf_session_id.clone();
        let call_invalidated = guard.call_binding.as_mut().map(|binding| {
            let invalidated_at = OffsetDateTime::now_utc();
            let call_id = binding.call_id.clone();
            binding.invalidated_at = Some(invalidated_at);
            binding.reason = Some("stop".to_string());
            (call_id, invalidated_at)
        });
        if let Some((call_id, invalidated_at)) = call_invalidated {
            if let Err(error) = guard.diagnostics.write(&DiagnosticRecord::CallInvalidated {
                qsf_session_id: stop_session_id.clone(),
                call_id,
                invalidated_at,
                reason: "stop".to_string(),
            }) {
                lifecycle_error.get_or_insert(error);
            }
        }
        guard.call_binding = None;
        (
            guard.sideband.take(),
            stop_session_id,
            completed,
            lifecycle_error,
        )
    };
    if let Some(sideband) = sideband {
        sideband.stop().await;
    }
    if let Some(error) = lifecycle_error {
        return Err(error);
    }
    Ok(StopResult {
        qsf_session_id: stop_session_id,
        completed_exchanges: completed,
    })
}

pub(crate) fn finalize_open_exchange(runtime: &mut SessionRuntime) {
    if let Some(active_exchange) = runtime.relay_state.active_exchange.as_ref() {
        let exchange_index = active_exchange.index;
        apply_relay_live_session_event(
            &mut runtime.relay_state,
            LiveSessionEvent::ExchangeCompleted {
                exchange_index,
                completed_at: std::time::SystemTime::now(),
            },
        );
    }
}

pub(crate) fn persist_completed_diagnostic_exchanges(
    runtime: &mut SessionRuntime,
) -> anyhow::Result<()> {
    while runtime.persisted_exchange_count < runtime.relay_state.completed_exchanges.len() {
        let exchange =
            runtime.relay_state.completed_exchanges[runtime.persisted_exchange_count].clone();
        runtime
            .diagnostics
            .write(&DiagnosticRecord::DiagnosticExchangeRecorded {
                qsf_session_id: runtime.qsf_session_id.clone(),
                source: "browser_relay".to_string(),
                trust: runtime.trust,
                recorded_at: OffsetDateTime::now_utc(),
                exchange,
            })?;
        runtime.persisted_exchange_count += 1;
    }
    Ok(())
}

fn apply_relay_live_session_event(
    state: &mut qsf_session::LiveSessionState,
    event: LiveSessionEvent,
) {
    *state = qsf_session::reduce_live_session(std::mem::take(state), event);
}
