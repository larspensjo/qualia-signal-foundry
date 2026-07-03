use std::convert::TryFrom;

use qsf_session::{
    ContinuityManifest, ResumeMode, SessionEvent, Turn, persist_session_state,
    reduce_session_in_place,
};
use time::OffsetDateTime;

use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust};
use crate::realtime::volition_continuity::{build_volition_continuity_snapshot, persist_snapshot};
use crate::state::{AppState, SessionRuntime};

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

        if runtime
            .non_promotable_exchange_indices
            .contains(&exchange.index)
        {
            log::info!(
                "trusted exchange `{}` for session `{}` skipped for continuity promotion because it was marked non-promotable",
                exchange.index,
                runtime.qsf_session_id
            );
            continue;
        }

        if runtime.degraded {
            log::warn!(
                "trusted exchange `{}` for session `{}` skipped for continuity promotion because sideband trust is degraded",
                exchange.index,
                runtime.qsf_session_id
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
            continue;
        };

        runtime
            .diagnostics
            .write(&DiagnosticRecord::DiagnosticExchangeRecorded {
                qsf_session_id: runtime.qsf_session_id.clone(),
                source: "sideband_trusted".to_string(),
                trust: DiagnosticTrust::Trusted,
                recorded_at: OffsetDateTime::now_utc(),
                exchange: exchange.clone(),
            })?;
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

        let continuity_dir = state.continuity_session_dir(&runtime.qsf_session_id);
        let state_path = persist_session_state(&runtime.session_state, &continuity_dir)?;
        let snapshot = build_volition_continuity_snapshot(
            &runtime.qsf_session_id,
            &runtime.volition,
            OffsetDateTime::now_utc(),
        )?;
        let snapshot_path = persist_snapshot(
            &snapshot,
            state.continuity_volition_snapshot_path(&runtime.qsf_session_id),
        )?;
        let mut manifest = ContinuityManifest::load_or_default(
            state.continuity_manifest_path(&runtime.qsf_session_id),
        )?;
        manifest.current_session_id = Some(runtime.qsf_session_id.clone());
        manifest.current_session_state_path = Some(
            state_path
                .strip_prefix(&continuity_dir)
                .unwrap_or(&state_path)
                .to_path_buf(),
        );
        manifest.current_volition_snapshot_path = Some(
            snapshot_path
                .strip_prefix(&continuity_dir)
                .unwrap_or(&snapshot_path)
                .to_path_buf(),
        );
        manifest.sleep_pending = true;
        manifest.resume_mode = ResumeMode::AwakeContinuation;
        manifest.persist(state.continuity_manifest_path(&runtime.qsf_session_id))?;
    }

    Ok(())
}
