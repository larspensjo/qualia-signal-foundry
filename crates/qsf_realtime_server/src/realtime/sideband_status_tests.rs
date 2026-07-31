use tempfile::TempDir;
use tokio::sync::mpsc;

use super::*;
use crate::realtime::sideband_provider_event::handle_provider_event;

#[tokio::test]
async fn set_sideband_status_notifies_subscribers() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let mut status_rx = runtime.lock().await.subscribe_status();
    assert!(!status_rx.borrow().degraded);

    runtime
        .lock()
        .await
        .set_sideband_status(true, Some("boom".to_string()));

    status_rx.changed().await.expect("status changed");
    let status = status_rx.borrow().clone();
    assert!(status.degraded);
    assert_eq!(status.detail.as_deref(), Some("boom"));
    assert!(runtime.lock().await.is_degraded());
}

#[tokio::test]
async fn session_updated_ack_clears_degraded_after_reconnect() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, _outbound_rx) = mpsc::unbounded_channel();
    {
        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        runtime
            .lock()
            .await
            .set_sideband_status(true, Some("test reconnect".to_string()));
    }

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        &browser_call("call-recovered"),
        "session.updated",
        &serde_json::json!({
            "type": "session.updated",
            "event_id": "evt-session-updated"
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("session updated");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    assert!(!runtime.lock().await.is_degraded());
    assert!(runtime.lock().await.is_sideband_attached());
}

#[tokio::test]
async fn disconnect_clears_sideband_attachment_readiness() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, _outbound_rx) = mpsc::unbounded_channel();

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        &browser_call("call-disconnect"),
        "session.updated",
        &serde_json::json!({ "type": "session.updated", "event_id": "evt-attached" }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("session updated");
    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        &browser_call("call-disconnect"),
        "session.closed",
        &serde_json::json!({ "type": "session.closed", "event_id": "evt-closed" }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("session closed");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let status = runtime.lock().await.subscribe_status().borrow().clone();
    assert!(!status.attached);
    assert!(status.degraded);
}

#[tokio::test]
async fn degradation_epoch_and_first_reasons_survive_recovery_before_read() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");

    {
        let mut guard = runtime.lock().await;
        guard.set_sideband_status(true, Some("first disconnect".to_string()));
        guard.set_sideband_status(false, None);
    }

    let guard = runtime.lock().await;
    let status = guard.subscribe_status().borrow().clone();
    assert!(!status.degraded, "current health should recover");
    assert!(status.degradation_epoch >= 1);
    assert_eq!(guard.degradation_epoch(), status.degradation_epoch);
    assert_eq!(guard.degradation_reasons(), ["first disconnect"]);
}

#[tokio::test]
async fn degradation_reason_retention_is_bounded_while_the_epoch_remains_monotonic() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");

    let mut guard = runtime.lock().await;
    for index in 0..(crate::state::MAX_DEGRADATION_REASONS + 2) {
        guard.set_sideband_status(true, Some(format!("disconnect-{index}")));
    }

    assert_eq!(
        guard.degradation_epoch() as usize,
        crate::state::MAX_DEGRADATION_REASONS + 2
    );
    assert_eq!(
        guard.degradation_reasons().len(),
        crate::state::MAX_DEGRADATION_REASONS
    );
    let last_retained_reason = format!("disconnect-{}", crate::state::MAX_DEGRADATION_REASONS - 1);
    assert_eq!(
        guard.degradation_reasons().last().map(String::as_str),
        Some(last_retained_reason.as_str())
    );
}

#[tokio::test]
async fn fail_closed_termination_is_monotonic_and_reportable() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");

    runtime
        .lock()
        .await
        .terminate_sideband("model session disconnected".to_string());
    let status = runtime.lock().await.subscribe_status().borrow().clone();
    assert!(
        status
            .terminated
            .as_deref()
            .is_some_and(|reason| reason.contains("model session disconnected"))
    );
    assert!(!status.attached);
    assert!(status.degraded);
}
