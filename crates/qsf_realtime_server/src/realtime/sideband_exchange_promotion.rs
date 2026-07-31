use std::convert::TryFrom;

use qsf_session::{SessionEvent, Turn, reduce_session_in_place};
use time::OffsetDateTime;

use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust};
use crate::realtime::volition_continuity::persist_continuity_state_and_volition_snapshot;
use crate::state::{AppState, SessionRuntime, TrustedTurnCompletion};

fn publish_completion(
    runtime: &SessionRuntime,
    exchange_index: usize,
    promoted: bool,
    promoted_turn_count: usize,
    skipped_reason: Option<String>,
) {
    runtime.publish_trusted_turn_completion(TrustedTurnCompletion {
        exchange_index,
        promoted,
        promoted_turn_count,
        skipped_reason,
        completed_at: OffsetDateTime::now_utc(),
    });
}

pub(super) async fn promote_completed_trusted_exchanges(
    state: &AppState,
    runtime: &mut SessionRuntime,
) -> anyhow::Result<()> {
    while runtime.trusted_promoted_exchange_count
        < runtime.session_state.live.completed_exchanges.len()
    {
        let exchange = runtime.session_state.live.completed_exchanges
            [runtime.trusted_promoted_exchange_count]
            .clone();
        runtime.trusted_promoted_exchange_count += 1;
        let promoted_turn_count_before = runtime.session_state.turns.len();

        if runtime
            .non_promotable_exchange_indices
            .contains(&exchange.index)
        {
            log::info!(
                "trusted exchange `{}` for session `{}` skipped for continuity promotion because it was marked non-promotable",
                exchange.index,
                runtime.qsf_session_id
            );
            publish_completion(
                runtime,
                exchange.index,
                false,
                promoted_turn_count_before,
                Some("non_promotable_exchange".to_string()),
            );
            continue;
        }

        if runtime.is_degraded() {
            log::warn!(
                "trusted exchange `{}` for session `{}` skipped for continuity promotion because sideband trust is degraded",
                exchange.index,
                runtime.qsf_session_id
            );
            publish_completion(
                runtime,
                exchange.index,
                false,
                promoted_turn_count_before,
                Some("sideband_degraded".to_string()),
            );
            continue;
        }

        let Ok(turn) = Turn::try_from(&exchange) else {
            log::warn!(
                "trusted exchange `{}` for session `{}` could not convert to a durable turn; skipping this exchange without degrading the session",
                exchange.index,
                runtime.qsf_session_id
            );
            runtime
                .non_promotable_exchange_indices
                .insert(exchange.index);
            publish_completion(
                runtime,
                exchange.index,
                false,
                promoted_turn_count_before,
                Some("turn_conversion_failed".to_string()),
            );
            continue;
        };

        if let Err(error) =
            runtime
                .diagnostics
                .write(&DiagnosticRecord::DiagnosticExchangeRecorded {
                    qsf_session_id: runtime.qsf_session_id.clone(),
                    source: "sideband_trusted".to_string(),
                    trust: DiagnosticTrust::Trusted,
                    recorded_at: OffsetDateTime::now_utc(),
                    exchange: exchange.clone(),
                })
        {
            publish_completion(
                runtime,
                exchange.index,
                false,
                promoted_turn_count_before,
                Some("diagnostic_persistence_failed".to_string()),
            );
            return Err(error);
        }
        log::info!(
            "trusted exchange `{}` for session `{}` recorded to diagnostics with {} tool request(s) and {} tool execution(s)",
            exchange.index,
            runtime.qsf_session_id,
            exchange.tool_requests.len(),
            exchange.tool_executions.len()
        );

        reduce_session_in_place(
            &mut runtime.session_state,
            SessionEvent::ExchangeRecorded {
                session_id: runtime.qsf_session_id.clone(),
                exchange: Box::new(exchange.clone()),
            },
        );
        reduce_session_in_place(
            &mut runtime.session_state,
            SessionEvent::TurnCompleted(turn),
        );

        if let Err(error) = persist_continuity_state_and_volition_snapshot(state, runtime) {
            publish_completion(
                runtime,
                exchange.index,
                false,
                promoted_turn_count_before,
                Some("continuity_persistence_failed".to_string()),
            );
            return Err(error);
        }
        publish_completion(
            runtime,
            exchange.index,
            true,
            runtime.session_state.turns.len(),
            None,
        );
    }

    Ok(())
}
