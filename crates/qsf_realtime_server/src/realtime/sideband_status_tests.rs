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
    assert!(runtime.lock().await.degraded);
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
        runtime.lock().await.degraded = true;
    }

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-recovered",
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
    assert!(!runtime.lock().await.degraded);
}
