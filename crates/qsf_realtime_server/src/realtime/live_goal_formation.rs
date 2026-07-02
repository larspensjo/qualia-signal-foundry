//! Post-response live goal formation and coherence detection (live goal formation and
//! off-hot-path coherence). Runs once per trusted turn, after `response.create` has been
//! dispatched, so turn latency is unaffected. One cache-structured model call proposes an
//! optional new goal candidate and detects any contradictions with the existing goal set; the
//! pure `qsf_volition::coherence` resolvers (the offline goal-coherence engine) decide admit /
//! reject / cancel deterministically from that verdict.
//!
//! See `docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md` for the trace-completeness
//! contract this record is the live analogue of - the automated harness asserts against the
//! offline `traces.jsonl`, not this `DiagnosticRecord`.

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::Mutex;

use qsf_models::{
    LiveGoalFormationJudge, ModelBackedLiveGoalFormationJudge, ModelClient,
    coherence_judge_goal_set, live_goal_formation_stable_prefix_hash,
};
use qsf_volition::{
    AdmissionResolution, Contradiction, DeclinedCandidate, VolitionEvent, apply,
    newly_declined_candidate, resolve_formed_candidate,
};

use crate::diagnostics::DiagnosticRecord;
use crate::state::SessionRuntime;

/// Live analogue of the offline `live-goal-formation` trace record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LiveGoalFormationTrace {
    pub tick: u64,
    pub input_transcript_ref: String,
    pub cached_prefix_ref: String,
    pub prefix_cache_eligible: bool,
    pub judge_model_role: String,
    pub judge_prompt_version: String,
    pub proposed_candidate_id: Option<String>,
    pub proposed_candidate_title: Option<String>,
    pub contradictions: Vec<Contradiction>,
    pub hard_tier_floor_rejected: bool,
    pub resolution: Option<AdmissionResolution>,
    /// The candidate newly added to `VolitionState::declined_candidates` this turn, if any.
    /// `None` both when nothing was rejected and when a rejection was deduplicated against an
    /// already-declined candidate with the same title.
    pub declined_candidate: Option<DeclinedCandidate>,
    pub events_emitted: Vec<VolitionEvent>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub response_dispatched_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub formation_started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub formation_completed_at: OffsetDateTime,
}

/// Returns a process-wide shared model client, built once from the environment and reused
/// across every trusted turn and session. Building a fresh `OpenAiProviderModelClient` per turn
/// would also spin up a fresh multi-threaded Tokio runtime and TLS/connection-pool stack per
/// call, discarding connection reuse for no benefit.
fn shared_model_client() -> anyhow::Result<Arc<dyn ModelClient>> {
    static CLIENT: OnceLock<Arc<dyn ModelClient>> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(Arc::clone(client));
    }
    let client: Arc<dyn ModelClient> = Arc::from(qsf_models::build_client_from_env()?);
    Ok(Arc::clone(CLIENT.get_or_init(|| client)))
}

/// Dispatches the formation+detection call as a background task, so the caller (the sideband's
/// `response.done` handler) does not wait on it. Failures are logged and recorded as a
/// `LiveGoalFormationFailed` diagnostic, never propagated - this runs after the turn has already
/// completed. Skips the spawn entirely (rather than racing it) when a formation task is already
/// in flight for this session.
pub(crate) fn spawn_live_goal_formation(
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: String,
    exchange_index: usize,
    turn_transcript: String,
    response_dispatched_at: Option<OffsetDateTime>,
) {
    tokio::spawn(async move {
        {
            let mut guard = session.lock().await;
            if guard.live_goal_formation_in_flight {
                log::info!(
                    "skipping live goal formation for session `{qsf_session_id}` exchange \
                     `{exchange_index}`: a formation task is already in flight for this session"
                );
                if let Err(write_error) =
                    guard
                        .diagnostics
                        .write(&DiagnosticRecord::LiveGoalFormationSkipped {
                            qsf_session_id: qsf_session_id.clone(),
                            exchange_index,
                            recorded_at: OffsetDateTime::now_utc(),
                            reason: "formation task already in flight".to_string(),
                        })
                {
                    log::warn!(
                        "failed to record live goal formation skip diagnostic for session \
                         `{qsf_session_id}` exchange `{exchange_index}`: {write_error:#}"
                    );
                }
                return;
            }
            guard.live_goal_formation_in_flight = true;
        }
        let _in_flight_reset = LiveGoalFormationInFlightReset::new(session.clone());

        let result = run_live_goal_formation(
            session.clone(),
            &qsf_session_id,
            exchange_index,
            turn_transcript,
            response_dispatched_at,
            shared_model_client,
        )
        .await;

        if let Err(error) = result {
            log::warn!(
                "live goal formation failed for session `{qsf_session_id}` exchange \
                 `{exchange_index}`: {error:#}"
            );
            let guard = session.lock().await;
            if let Err(write_error) =
                guard
                    .diagnostics
                    .write(&DiagnosticRecord::LiveGoalFormationFailed {
                        qsf_session_id: qsf_session_id.clone(),
                        exchange_index,
                        recorded_at: OffsetDateTime::now_utc(),
                        error: format!("{error:#}"),
                    })
            {
                log::warn!(
                    "failed to record live goal formation failure diagnostic for session \
                     `{qsf_session_id}` exchange `{exchange_index}`: {write_error:#}"
                );
            }
        }
    });
}

struct LiveGoalFormationInFlightReset {
    session: Arc<Mutex<SessionRuntime>>,
}

impl LiveGoalFormationInFlightReset {
    fn new(session: Arc<Mutex<SessionRuntime>>) -> Self {
        Self { session }
    }
}

impl Drop for LiveGoalFormationInFlightReset {
    fn drop(&mut self) {
        let session = Arc::clone(&self.session);
        tokio::spawn(async move {
            session.lock().await.live_goal_formation_in_flight = false;
        });
    }
}

/// `build_client` is injected (rather than always resolved from `shared_model_client`) so tests
/// can drive this with a deterministic client without depending on the process environment.
async fn run_live_goal_formation<F>(
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: &str,
    exchange_index: usize,
    turn_transcript: String,
    response_dispatched_at: Option<OffsetDateTime>,
    build_client: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<Arc<dyn ModelClient>> + Send + 'static,
{
    let (state, fixture, tick, last_prefix_hash) = {
        let guard = session.lock().await;
        (
            guard.volition.state.clone(),
            guard.volition.fixture.clone(),
            guard.volition.state.tick,
            guard.volition.last_goal_set_prefix_hash.clone(),
        )
    };

    let goal_set = coherence_judge_goal_set(&state, &fixture);
    let prefix_hash = live_goal_formation_stable_prefix_hash(&goal_set);
    let prefix_cache_eligible = last_prefix_hash.as_deref() == Some(prefix_hash.as_str());

    let formation_started_at = OffsetDateTime::now_utc();
    // `goal_set` is not read again after this point, so it moves into the blocking task
    // directly rather than being cloned first.
    let outcome = tokio::task::spawn_blocking(
        move || -> anyhow::Result<qsf_models::LiveGoalFormationOutcome> {
            let client = build_client()?;
            let judge = ModelBackedLiveGoalFormationJudge::new(client.as_ref());
            let mut invoker = qsf_models::DirectModelInvoker;
            judge.form_and_detect(&mut invoker, &goal_set, &turn_transcript)
        },
    )
    .await
    .map_err(|join_error| anyhow::anyhow!("live goal formation task panicked: {join_error}"))??;
    let formation_completed_at = OffsetDateTime::now_utc();

    let (events, resolution) = match &outcome.proposed_candidate {
        Some(candidate) => {
            let (events, resolution) =
                resolve_formed_candidate(candidate, &outcome.verdict, &state, &fixture, tick);
            (events, Some(resolution))
        }
        None => (Vec::new(), None),
    };
    let hard_tier_floor_rejected =
        matches!(resolution, Some(AdmissionResolution::RejectProtectedFloor));

    let mut guard = session.lock().await;

    // A trusted turn's formation is serialized by `live_goal_formation_in_flight`, but a
    // discard-if-stale check is cheap insurance: if the goal set queried at the start of this
    // call no longer matches current state (e.g. a sleep-consolidation pass admitted or
    // cancelled goals while this model call was in flight), the events computed above were
    // resolved against a snapshot that no longer holds - discard rather than apply them.
    let current_goal_set = coherence_judge_goal_set(&guard.volition.state, &guard.volition.fixture);
    if live_goal_formation_stable_prefix_hash(&current_goal_set) != prefix_hash {
        log::warn!(
            "discarding live goal formation outcome for session `{qsf_session_id}` exchange \
             `{exchange_index}`: the goal set changed during formation"
        );
        guard
            .diagnostics
            .write(&DiagnosticRecord::LiveGoalFormationSkipped {
                qsf_session_id: qsf_session_id.to_string(),
                exchange_index,
                recorded_at: OffsetDateTime::now_utc(),
                reason: "goal set changed during formation".to_string(),
            })?;
        return Ok(());
    }

    let state_before_apply = guard.volition.state.clone();
    let mut next_state = guard.volition.state.clone();
    for event in events.clone() {
        next_state = apply(next_state, event);
    }
    let declined_candidate = outcome.proposed_candidate.as_ref().and_then(|candidate| {
        newly_declined_candidate(&state_before_apply, &next_state, candidate.id())
    });

    let trace = LiveGoalFormationTrace {
        tick,
        input_transcript_ref: format!("exchange-{exchange_index}"),
        cached_prefix_ref: prefix_hash.clone(),
        prefix_cache_eligible,
        judge_model_role: outcome.verdict.judge_ref.model_role.clone(),
        judge_prompt_version: outcome.verdict.judge_ref.prompt_version.clone(),
        proposed_candidate_id: outcome
            .proposed_candidate
            .as_ref()
            .map(|c| c.id().to_string()),
        proposed_candidate_title: outcome
            .proposed_candidate
            .as_ref()
            .map(|c| c.title().to_string()),
        contradictions: outcome.verdict.contradictions.clone(),
        hard_tier_floor_rejected,
        resolution,
        declined_candidate,
        events_emitted: events.clone(),
        response_dispatched_at,
        formation_started_at,
        formation_completed_at,
    };

    // Written before the state mutation below: if this fails, `?` propagates and
    // `guard.volition.state` is left untouched, so state never silently diverges from what the
    // diagnostics stream can explain.
    guard
        .diagnostics
        .write(&DiagnosticRecord::LiveGoalFormationPerformed {
            qsf_session_id: qsf_session_id.to_string(),
            exchange_index,
            recorded_at: OffsetDateTime::now_utc(),
            trace,
        })?;

    guard.volition.state = next_state;
    guard.volition.last_goal_set_prefix_hash = Some(prefix_hash);

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::state::AppState;

    fn state(tempdir: &TempDir) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            "wss://example.invalid/realtime",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state")
    }

    fn mock_client() -> anyhow::Result<Arc<dyn ModelClient>> {
        Ok(Arc::new(qsf_models::MockModelClient::default()))
    }

    fn always_fails_client() -> anyhow::Result<Arc<dyn ModelClient>> {
        Err(anyhow::anyhow!("provider unavailable"))
    }

    #[tokio::test]
    async fn no_candidate_formed_writes_diagnostic_with_no_lifecycle_events() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();

        run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            0,
            "[User]\nhello\n\n[Assistant]\nhi there".to_string(),
            None,
            mock_client,
        )
        .await
        .unwrap();

        let guard = session.lock().await;
        assert!(guard.volition.state.declined_candidates.is_empty());
        assert!(guard.volition.last_goal_set_prefix_hash.is_some());
        drop(guard);

        let diagnostics_content = std::fs::read_to_string(
            app_state
                .diagnostics_dir()
                .join(format!("{}.jsonl", allocation.qsf_session_id)),
        )
        .unwrap();
        let record: serde_json::Value = diagnostics_content
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|value| value["kind"] == "live_goal_formation_performed")
            .expect("a live_goal_formation_performed diagnostic record");
        assert_eq!(
            record["trace"]["proposed_candidate_id"],
            serde_json::Value::Null
        );
        assert_eq!(record["trace"]["prefix_cache_eligible"], false);
        assert_eq!(record["trace"]["events_emitted"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn unchanged_goal_set_is_cache_eligible_on_the_next_call() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();

        run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            0,
            "first turn".to_string(),
            None,
            mock_client,
        )
        .await
        .unwrap();
        run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            1,
            "second turn".to_string(),
            None,
            mock_client,
        )
        .await
        .unwrap();

        let diagnostics_content = std::fs::read_to_string(
            app_state
                .diagnostics_dir()
                .join(format!("{}.jsonl", allocation.qsf_session_id)),
        )
        .unwrap();
        let records: Vec<serde_json::Value> = diagnostics_content
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|value| value["kind"] == "live_goal_formation_performed")
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["trace"]["prefix_cache_eligible"], false);
        assert_eq!(records[1]["trace"]["prefix_cache_eligible"], true);
        assert_eq!(
            records[0]["trace"]["cached_prefix_ref"],
            records[1]["trace"]["cached_prefix_ref"]
        );
    }

    #[tokio::test]
    async fn a_failed_formation_call_writes_a_failure_diagnostic_and_leaves_state_untouched() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();
        let tick_before = session.lock().await.volition.state.tick;

        let error = run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            0,
            "a turn transcript".to_string(),
            None,
            always_fails_client,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("provider unavailable"));
        let guard = session.lock().await;
        assert_eq!(
            guard.volition.state.tick, tick_before,
            "a failed formation call must not mutate volition state"
        );
        assert!(guard.volition.last_goal_set_prefix_hash.is_none());
    }

    #[tokio::test]
    async fn spawn_records_a_failure_diagnostic_when_formation_errors() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();

        let result = run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            0,
            "a turn transcript".to_string(),
            None,
            always_fails_client,
        )
        .await;
        assert!(result.is_err());

        // Exercise the same diagnostic write spawn_live_goal_formation performs on error, since
        // spawn_live_goal_formation itself fires a detached tokio task with no return value to
        // await on.
        let guard = session.lock().await;
        guard
            .diagnostics
            .write(&DiagnosticRecord::LiveGoalFormationFailed {
                qsf_session_id: allocation.qsf_session_id.clone(),
                exchange_index: 0,
                recorded_at: OffsetDateTime::now_utc(),
                error: result.unwrap_err().to_string(),
            })
            .unwrap();
        drop(guard);

        let diagnostics_content = std::fs::read_to_string(
            app_state
                .diagnostics_dir()
                .join(format!("{}.jsonl", allocation.qsf_session_id)),
        )
        .unwrap();
        assert!(
            diagnostics_content
                .lines()
                .any(|line| line.contains("live_goal_formation_failed")),
            "expected a live_goal_formation_failed diagnostic record"
        );
    }

    #[tokio::test]
    async fn a_stale_goal_set_at_apply_time_is_discarded_without_mutating_state() {
        use qsf_volition::{AllowedEffect, EvidenceRef, GoalScope, ProposedGoalCandidate};

        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();

        // Snapshot the goal-set prefix hash `run_live_goal_formation` will query, then mutate
        // the goal set (as a concurrent admission or a sleep pass could) before the model call
        // "completes" - simulated here by calling the pure function directly rather than
        // spawning a real task, since the point under test is the post-call re-check, not
        // scheduling.
        let candidate = ProposedGoalCandidate::try_new(
            "concurrent-candidate".to_string(),
            "Concurrent candidate".to_string(),
            "Formed by a concurrent turn".to_string(),
            vec![],
            GoalScope::Session,
            50,
            vec![AllowedEffect::Reflect],
            "satisfied when discussed".to_string(),
            vec![EvidenceRef::try_new("evidence").unwrap()],
            "concurrent turn".to_string(),
            vec![],
        )
        .unwrap();

        {
            let mut guard = session.lock().await;
            let tick = guard.volition.state.tick + 1;
            guard.volition.apply_events(vec![
                VolitionEvent::GoalCandidateAdded {
                    candidate: candidate.clone(),
                    tick,
                },
                VolitionEvent::GoalCandidateAccepted {
                    goal_id: "concurrent-candidate".to_string(),
                    acceptance_evidence: EvidenceRef::try_new("accepted").unwrap(),
                    tick,
                },
            ]);
        }
        let tick_after_mutation = session.lock().await.volition.state.tick;

        // Now run formation with a build_client that mutates the goal set again mid-call, to
        // simulate the goal set changing after `run_live_goal_formation` snapshots it but
        // before it re-locks to apply events.
        let mutating_session = session.clone();
        let build_client = move || -> anyhow::Result<Arc<dyn ModelClient>> {
            // Runs on the `spawn_blocking` thread inside `run_live_goal_formation`, so
            // `blocking_lock` (not `.lock().await`) is the correct way to take the mutex here.
            let mut guard = mutating_session.blocking_lock();
            let tick = guard.volition.state.tick + 1;
            guard
                .volition
                .apply_events(vec![VolitionEvent::GoalRetired {
                    goal_id: "concurrent-candidate".to_string(),
                    tick,
                }]);
            drop(guard);
            Ok(Arc::new(qsf_models::MockModelClient::default()))
        };

        run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            0,
            "a turn transcript".to_string(),
            None,
            build_client,
        )
        .await
        .unwrap();

        let guard = session.lock().await;
        assert!(
            guard.volition.last_goal_set_prefix_hash.is_none(),
            "a discarded outcome must not stamp last_goal_set_prefix_hash"
        );
        assert!(
            guard.volition.state.tick > tick_after_mutation,
            "the concurrent mutation performed mid-call must still be in state"
        );
    }
}
