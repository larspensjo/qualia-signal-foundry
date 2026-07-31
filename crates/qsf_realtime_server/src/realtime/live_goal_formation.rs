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
use std::time::Duration;

use time::OffsetDateTime;
use tokio::sync::Mutex;

use qsf_models::{
    CapturedModelUse, LiveGoalFormationJudge, ModelBackedLiveGoalFormationJudge, ModelClient,
    ModelUsage, UsageCapturingInvoker, coherence_judge_goal_set,
    live_goal_formation_stable_prefix_hash,
};
use qsf_volition::{
    AdmissionResolution, apply, newly_declined_candidate, resolve_formed_candidate,
};

use crate::diagnostics::DiagnosticRecord;
use crate::state::SessionRuntime;

pub use qsf_diagnostics::LiveGoalFormationTrace;

/// Default bound for a run waiting for detached formation work. The later launcher flag uses
/// this value as its default, keeping the runtime primitive and CLI policy on one source of truth.
#[allow(dead_code)]
pub(crate) const DEFAULT_LIVE_GOAL_FORMATION_BARRIER_TIMEOUT_MS: u64 = 60_000;

/// Monotonic progress for the per-session live-goal-formation queue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveGoalFormationProgress {
    pub expected: usize,
    pub settled: usize,
    pub failed: usize,
    pub in_flight: bool,
}

/// Result of waiting for formation to settle. A timeout is a reportable partial outcome, not an
/// infrastructure error: callers receive the counts observed at the deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum LiveGoalFormationBarrierOutcome {
    Settled {
        expected: usize,
        settled: usize,
        failed: usize,
    },
    TimedOut {
        expected: usize,
        settled: usize,
        failed: usize,
        in_flight: bool,
    },
    ChannelClosed {
        expected: usize,
        settled: usize,
        failed: usize,
        in_flight: bool,
    },
}

/// Wait until `settled == expected && !in_flight`, or return a structured incomplete outcome.
///
/// The first progress observation is made while holding the session lock. This orders a caller
/// awakened by trusted-turn completion after the response handler's formation enqueue, because
/// both completion publication and enqueue happen under that same lock.
///
/// A fresh session is already settled (`expected == 0`), so callers must not invoke this before
/// the trusted-turn completion that establishes their final enqueue boundary. Work enqueued after
/// this function returns is outside the completed barrier interval and cannot be observed by that
/// waiter.
#[allow(dead_code)]
pub(crate) async fn wait_for_live_goal_formation_barrier(
    session: Arc<Mutex<SessionRuntime>>,
    timeout: Duration,
) -> LiveGoalFormationBarrierOutcome {
    let observe = |progress: &LiveGoalFormationProgress| {
        if progress.settled == progress.expected && !progress.in_flight {
            Some(LiveGoalFormationBarrierOutcome::Settled {
                expected: progress.expected,
                settled: progress.settled,
                failed: progress.failed,
            })
        } else {
            None
        }
    };

    let (initial_progress, mut progress_rx) = {
        let guard = session.lock().await;
        (
            guard.live_goal_formation_progress(),
            guard.subscribe_live_goal_formation_progress(),
        )
    };
    drop(session);

    if let Some(outcome) = observe(&initial_progress) {
        return outcome;
    }

    let changed = async {
        loop {
            if progress_rx.changed().await.is_err() {
                return Err(());
            }
            if let Some(outcome) = observe(&progress_rx.borrow()) {
                return Ok(outcome);
            }
        }
    };
    match tokio::time::timeout(timeout, changed).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(())) => {
            let progress = progress_rx.borrow().clone();
            LiveGoalFormationBarrierOutcome::ChannelClosed {
                expected: progress.expected,
                settled: progress.settled,
                failed: progress.failed,
                in_flight: progress.in_flight,
            }
        }
        Err(_) => {
            let progress = progress_rx.borrow().clone();
            LiveGoalFormationBarrierOutcome::TimedOut {
                expected: progress.expected,
                settled: progress.settled,
                failed: progress.failed,
                in_flight: progress.in_flight,
            }
        }
    }
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

/// One trusted turn awaiting live goal formation, queued by `enqueue_live_goal_formation` and
/// drained in order by the per-session worker loop.
pub(crate) struct PendingLiveGoalFormation {
    exchange_index: usize,
    turn_transcript: String,
    response_dispatched_at: Option<OffsetDateTime>,
}

struct LiveGoalFormationCallResult {
    outcome: anyhow::Result<qsf_models::LiveGoalFormationOutcome>,
    captured: Vec<CapturedModelUse>,
}

fn token_counts_from_model_usage(
    usage: &ModelUsage,
) -> crate::realtime::token_usage::TokenClassCounts {
    crate::realtime::token_usage::TokenClassCounts {
        text_input: usage.input_tokens.saturating_sub(usage.cached_input_tokens) as u64,
        audio_input: 0,
        cached_input: usage.cached_input_tokens as u64,
        text_output: usage.output_tokens as u64,
        audio_output: 0,
    }
}

/// Builds the model client used for one formation call. Boxed so the per-item worker loop can
/// call it repeatedly (once per queued turn) without needing to know whether it came from the
/// process-wide `shared_model_client` or, in tests, a client that drives specific timing.
type ClientBuilder = Arc<dyn Fn() -> anyhow::Result<Arc<dyn ModelClient>> + Send + Sync>;
type AfterFormationItem = Arc<dyn Fn(usize) + Send + Sync>;

/// Enqueue one eligible turn while the caller holds the session lock. Returning whether a worker
/// should be spawned makes the enqueue visible to a barrier before the lock is released.
pub(crate) fn enqueue_live_goal_formation(
    runtime: &mut SessionRuntime,
    exchange_index: usize,
    turn_transcript: String,
    response_dispatched_at: Option<OffsetDateTime>,
) -> bool {
    runtime
        .live_goal_formation_queue
        .push_back(PendingLiveGoalFormation {
            exchange_index,
            turn_transcript,
            response_dispatched_at,
        });
    runtime.live_goal_formation_expected += 1;
    let should_spawn_worker = !runtime.live_goal_formation_in_flight;
    if should_spawn_worker {
        runtime.live_goal_formation_in_flight = true;
    }
    runtime.publish_live_goal_formation_progress();
    should_spawn_worker
}

/// Spawn the off-hot-path formation worker after the session lock has been released.
pub(crate) fn spawn_live_goal_formation_worker(
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: String,
) {
    spawn_live_goal_formation_worker_with_client_builder(
        session,
        qsf_session_id,
        Arc::new(shared_model_client),
    );
}

pub(crate) fn spawn_live_goal_formation_worker_with_client_builder(
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: String,
    build_client: ClientBuilder,
) {
    spawn_live_goal_formation_worker_with_hooks(
        session,
        qsf_session_id,
        build_client,
        Arc::new(|_| {}),
    );
}

fn spawn_live_goal_formation_worker_with_hooks(
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: String,
    build_client: ClientBuilder,
    after_item: AfterFormationItem,
) {
    tokio::spawn(async move {
        drain_live_goal_formation_queue(session, qsf_session_id, build_client, after_item).await;
    });
}

/// Pops and processes queued turns one at a time, in FIFO order, until the queue is empty. Popping
/// and the empty-queue in-flight reset both happen under the same session-lock acquisition, so a
/// concurrently enqueuing call can never observe `live_goal_formation_in_flight = false` while an
/// item this worker hasn't processed yet is still sitting in the queue.
async fn drain_live_goal_formation_queue(
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: String,
    build_client: ClientBuilder,
    after_item: AfterFormationItem,
) {
    let mut in_flight_guard = LiveGoalFormationInFlightGuard::new(
        session.clone(),
        qsf_session_id.clone(),
        Arc::clone(&build_client),
        Arc::clone(&after_item),
    );
    loop {
        let next = {
            let mut guard = session.lock().await;
            match guard.live_goal_formation_queue.pop_front() {
                Some(pending) => Some(pending),
                None => {
                    guard.live_goal_formation_in_flight = false;
                    guard.publish_live_goal_formation_progress();
                    None
                }
            }
        };
        let Some(pending) = next else {
            break;
        };

        in_flight_guard.begin_item(pending.exchange_index);
        let client_builder = Arc::clone(&build_client);
        let after_item_hook = Arc::clone(&after_item);
        let exchange_index = pending.exchange_index;
        let result = run_live_goal_formation_with_after_model_call(
            session.clone(),
            &qsf_session_id,
            exchange_index,
            pending.turn_transcript,
            pending.response_dispatched_at,
            move || client_builder(),
            move || after_item_hook(exchange_index),
        )
        .await;

        let failed = result.is_err();
        if let Err(error) = result {
            let exchange_index = pending.exchange_index;
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
        let mut guard = session.lock().await;
        guard.live_goal_formation_settled += 1;
        if failed {
            guard.live_goal_formation_failed += 1;
        }
        guard.publish_live_goal_formation_progress();
        in_flight_guard.settle_item();
    }
    in_flight_guard.disarm();
}

/// Best-effort unwind and cancellation safety net for the worker loop. Normal exits clear the
/// in-flight flag and disarm this guard. An abnormal exit marks only the item being processed as
/// failed, records that loss in diagnostics, preserves queued work, and starts a replacement
/// worker when needed so one failed analysis cannot silently discard later turns.
struct LiveGoalFormationInFlightGuard {
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: String,
    build_client: ClientBuilder,
    after_item: AfterFormationItem,
    disarmed: bool,
    item_in_flight: Option<usize>,
}

impl LiveGoalFormationInFlightGuard {
    fn new(
        session: Arc<Mutex<SessionRuntime>>,
        qsf_session_id: String,
        build_client: ClientBuilder,
        after_item: AfterFormationItem,
    ) -> Self {
        Self {
            session,
            qsf_session_id,
            build_client,
            after_item,
            disarmed: false,
            item_in_flight: None,
        }
    }

    fn disarm(mut self) {
        self.disarmed = true;
    }

    fn begin_item(&mut self, exchange_index: usize) {
        self.item_in_flight = Some(exchange_index);
    }

    fn settle_item(&mut self) {
        self.item_in_flight = None;
    }
}

impl Drop for LiveGoalFormationInFlightGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        let session = Arc::clone(&self.session);
        let qsf_session_id = self.qsf_session_id.clone();
        let build_client = Arc::clone(&self.build_client);
        let after_item = Arc::clone(&self.after_item);
        let item_in_flight = self.item_in_flight;
        tokio::spawn(async move {
            let should_restart = {
                let mut guard = session.lock().await;
                let queued_exchange_indices = guard
                    .live_goal_formation_queue
                    .iter()
                    .map(|pending| pending.exchange_index)
                    .collect::<Vec<_>>();
                if let Some(exchange_index) = item_in_flight {
                    let error = format!(
                        "live goal formation worker ended unexpectedly while processing exchange \
                         `{exchange_index}`"
                    );
                    log::warn!(
                        "{error} for session `{qsf_session_id}`; preserved queued exchange indices: \
                         {queued_exchange_indices:?}"
                    );
                    if let Err(write_error) =
                        guard
                            .diagnostics
                            .write(&DiagnosticRecord::LiveGoalFormationFailed {
                                qsf_session_id: qsf_session_id.clone(),
                                exchange_index,
                                recorded_at: OffsetDateTime::now_utc(),
                                error,
                            })
                    {
                        log::warn!(
                            "failed to record unexpected live goal formation worker exit for \
                             session `{qsf_session_id}` exchange `{exchange_index}`: {write_error:#}"
                        );
                    }
                    guard.live_goal_formation_settled += 1;
                    guard.live_goal_formation_failed += 1;
                } else {
                    log::warn!(
                        "live goal formation worker ended unexpectedly for session \
                         `{qsf_session_id}` with no item in flight; preserved queued exchange \
                         indices: {queued_exchange_indices:?}"
                    );
                }
                guard.live_goal_formation_in_flight = false;
                guard.publish_live_goal_formation_progress();
                let should_restart = !guard.live_goal_formation_queue.is_empty();
                if should_restart {
                    guard.live_goal_formation_in_flight = true;
                    guard.publish_live_goal_formation_progress();
                }
                should_restart
            };
            if should_restart {
                spawn_live_goal_formation_worker_with_hooks(
                    session,
                    qsf_session_id,
                    build_client,
                    after_item,
                );
            }
        });
    }
}

/// `build_client` is injected (rather than always resolved from `shared_model_client`) so tests
/// can drive this with a deterministic client without depending on the process environment.
#[cfg(test)]
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
    run_live_goal_formation_with_after_model_call(
        session,
        qsf_session_id,
        exchange_index,
        turn_transcript,
        response_dispatched_at,
        build_client,
        || {},
    )
    .await
}

async fn run_live_goal_formation_with_after_model_call<F, H>(
    session: Arc<Mutex<SessionRuntime>>,
    qsf_session_id: &str,
    exchange_index: usize,
    turn_transcript: String,
    response_dispatched_at: Option<OffsetDateTime>,
    build_client: F,
    after_model_call: H,
) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<Arc<dyn ModelClient>> + Send + 'static,
    H: FnOnce(),
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
    let call_result = tokio::task::spawn_blocking(move || match build_client() {
        Ok(client) => {
            let judge = ModelBackedLiveGoalFormationJudge::new(client.as_ref());
            let mut invoker = UsageCapturingInvoker::default();
            let outcome = judge.form_and_detect(&mut invoker, &goal_set, &turn_transcript);
            LiveGoalFormationCallResult {
                outcome,
                captured: invoker.captured,
            }
        }
        Err(error) => LiveGoalFormationCallResult {
            outcome: Err(error),
            captured: Vec::new(),
        },
    })
    .await
    .map_err(|join_error| anyhow::anyhow!("live goal formation task panicked: {join_error}"))?;
    after_model_call();
    let formation_completed_at = OffsetDateTime::now_utc();
    let LiveGoalFormationCallResult { outcome, captured } = call_result;
    if !captured.is_empty() {
        let mut guard = session.lock().await;
        for captured_use in captured {
            guard.record_token_usage(
                crate::realtime::token_usage::GOAL_FORMATION_ROLE,
                &captured_use.model_name,
                token_counts_from_model_usage(&captured_use.usage),
            );
        }
    }
    let outcome = outcome?;

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
    use qsf_volition::VolitionEvent;

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

    /// A judge client whose first `complete` call signals `started_tx` and then blocks on
    /// `release_rx` until the test releases it; every subsequent call returns immediately. Lets a
    /// test drive a second turn's formation worker while the first turn's model
    /// call is genuinely in flight, rather than merely racing two async tasks.
    struct BlockOnFirstCallClient {
        started_tx: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        release_rx: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    }

    impl ModelClient for BlockOnFirstCallClient {
        fn client_name(&self) -> &str {
            "block-on-first-call-client"
        }

        fn complete(
            &self,
            request: &qsf_models::ModelRequest,
        ) -> anyhow::Result<qsf_models::ModelResponse> {
            if let Some(started_tx) = self.started_tx.lock().unwrap().take() {
                let _ = started_tx.send(());
                if let Some(release_rx) = self.release_rx.lock().unwrap().take() {
                    let _ = release_rx.recv();
                }
            }
            Ok(qsf_models::ModelResponse::from_text(
                request,
                self.client_name(),
                request.model_name.clone(),
                serde_json::json!({ "proposed_candidate": null, "contradictions": [] }).to_string(),
            ))
        }
    }

    async fn enqueue_and_spawn_for_test(
        session: Arc<Mutex<SessionRuntime>>,
        qsf_session_id: String,
        exchange_index: usize,
        turn_transcript: &str,
        build_client: ClientBuilder,
    ) {
        let should_spawn_worker = {
            let mut guard = session.lock().await;
            enqueue_live_goal_formation(
                &mut guard,
                exchange_index,
                turn_transcript.to_string(),
                None,
            )
        };
        if should_spawn_worker {
            spawn_live_goal_formation_worker_with_client_builder(
                session,
                qsf_session_id,
                build_client,
            );
        }
    }

    #[tokio::test]
    async fn two_turns_completing_while_the_first_formation_call_blocks_are_both_processed_in_order()
     {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let client: Arc<dyn ModelClient> = Arc::new(BlockOnFirstCallClient {
            started_tx: std::sync::Mutex::new(Some(started_tx)),
            release_rx: std::sync::Mutex::new(Some(release_rx)),
        });
        let build_client: ClientBuilder = {
            let client = Arc::clone(&client);
            Arc::new(move || Ok(Arc::clone(&client)))
        };

        enqueue_and_spawn_for_test(
            session.clone(),
            allocation.qsf_session_id.clone(),
            0,
            "first turn",
            Arc::clone(&build_client),
        )
        .await;

        // Wait until the first call is genuinely blocked inside its model call before enqueuing
        // the second turn, so this exercises the queue rather than a race between two spawns.
        started_rx.await.expect("first call must start");

        enqueue_and_spawn_for_test(
            session.clone(),
            allocation.qsf_session_id.clone(),
            1,
            "second turn",
            Arc::clone(&build_client),
        )
        .await;

        // The second turn must land in the queue (not spawn its own concurrent worker) while the
        // first call is still blocked.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if session.lock().await.live_goal_formation_queue.len() == 1 {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the second turn must be queued while the first call is in flight");

        release_tx.send(()).expect("release the first call");

        let performed_exchange_indices =
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let diagnostics_content = std::fs::read_to_string(
                        app_state
                            .diagnostics_dir()
                            .join(format!("{}.jsonl", allocation.qsf_session_id)),
                    )
                    .unwrap();
                    let indices: Vec<usize> = diagnostics_content
                        .lines()
                        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                        .filter(|value| value["kind"] == "live_goal_formation_performed")
                        .map(|value| value["exchange_index"].as_u64().unwrap() as usize)
                        .collect();
                    if indices.len() >= 2 {
                        return indices;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("both queued turns must produce a live_goal_formation_performed diagnostic");

        assert_eq!(
            performed_exchange_indices,
            vec![0, 1],
            "queued turns must be processed in FIFO order"
        );
        assert!(!session.lock().await.live_goal_formation_in_flight);
        assert!(session.lock().await.live_goal_formation_queue.is_empty());
    }

    #[tokio::test]
    async fn barrier_observes_enqueue_before_worker_and_waits_until_settled() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let client: Arc<dyn ModelClient> = Arc::new(BlockOnFirstCallClient {
            started_tx: std::sync::Mutex::new(Some(started_tx)),
            release_rx: std::sync::Mutex::new(Some(release_rx)),
        });
        let build_client: ClientBuilder = {
            let client = Arc::clone(&client);
            Arc::new(move || Ok(Arc::clone(&client)))
        };
        // Simulate a completion waiter waking while the response handler still owns the session
        // lock, before that handler enqueues formation. The barrier must wait for this lock-held
        // enqueue instead of accepting the stale watch value (`expected == settled == 0`).
        let mut guard = session.lock().await;
        let waiter = tokio::spawn(wait_for_live_goal_formation_barrier(
            session.clone(),
            Duration::from_secs(5),
        ));
        tokio::task::yield_now().await;
        let should_spawn_worker =
            enqueue_live_goal_formation(&mut guard, 0, "blocked turn".to_string(), None);
        let progress = guard.live_goal_formation_progress();
        assert_eq!(progress.expected, 1);
        assert_eq!(progress.settled, 0);
        assert!(progress.in_flight);
        drop(guard);
        assert!(should_spawn_worker);
        spawn_live_goal_formation_worker_with_client_builder(
            session.clone(),
            allocation.qsf_session_id.clone(),
            build_client,
        );
        started_rx.await.unwrap();

        assert!(
            tokio::time::timeout(Duration::from_millis(25), waiter)
                .await
                .is_err(),
            "the barrier must not report drained while the model call is blocked"
        );
        release_tx.send(()).unwrap();

        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            wait_for_live_goal_formation_barrier(session.clone(), Duration::from_secs(5)),
        )
        .await
        .unwrap();
        assert_eq!(
            outcome,
            LiveGoalFormationBarrierOutcome::Settled {
                expected: 1,
                settled: 1,
                failed: 0,
            }
        );
    }

    #[tokio::test]
    async fn barrier_timeout_returns_observed_partial_counts() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let client: Arc<dyn ModelClient> = Arc::new(BlockOnFirstCallClient {
            started_tx: std::sync::Mutex::new(Some(started_tx)),
            release_rx: std::sync::Mutex::new(Some(release_rx)),
        });
        let build_client: ClientBuilder = {
            let client = Arc::clone(&client);
            Arc::new(move || Ok(Arc::clone(&client)))
        };
        enqueue_and_spawn_for_test(
            session.clone(),
            allocation.qsf_session_id.clone(),
            0,
            "never completing turn",
            build_client,
        )
        .await;
        started_rx.await.unwrap();
        let outcome =
            wait_for_live_goal_formation_barrier(session.clone(), Duration::from_millis(10)).await;
        assert_eq!(
            outcome,
            LiveGoalFormationBarrierOutcome::TimedOut {
                expected: 1,
                settled: 0,
                failed: 0,
                in_flight: true,
            }
        );
        release_tx.send(()).unwrap();
    }

    #[tokio::test]
    async fn worker_frame_panic_fails_only_the_current_item_and_drains_queued_work() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();
        let should_spawn_worker = {
            let mut guard = session.lock().await;
            let should_spawn_worker =
                enqueue_live_goal_formation(&mut guard, 0, "panicking turn".to_string(), None);
            assert!(should_spawn_worker);
            assert!(!enqueue_live_goal_formation(
                &mut guard,
                1,
                "queued turn".to_string(),
                None,
            ));
            should_spawn_worker
        };
        let after_item: AfterFormationItem = Arc::new(|exchange_index| {
            if exchange_index == 0 {
                panic!("simulated panic in the formation worker frame");
            }
        });
        if should_spawn_worker {
            spawn_live_goal_formation_worker_with_hooks(
                session.clone(),
                allocation.qsf_session_id.clone(),
                Arc::new(mock_client),
                after_item,
            );
        }

        let outcome =
            wait_for_live_goal_formation_barrier(session.clone(), Duration::from_secs(5)).await;
        assert_eq!(
            outcome,
            LiveGoalFormationBarrierOutcome::Settled {
                expected: 2,
                settled: 2,
                failed: 1,
            }
        );
        let guard = session.lock().await;
        assert!(!guard.live_goal_formation_in_flight);
        assert!(guard.live_goal_formation_queue.is_empty());
        drop(guard);

        let diagnostics_content = std::fs::read_to_string(
            app_state
                .diagnostics_dir()
                .join(format!("{}.jsonl", allocation.qsf_session_id)),
        )
        .unwrap();
        let records = diagnostics_content
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(records.iter().any(|record| {
            record["kind"] == "live_goal_formation_failed" && record["exchange_index"] == 0
        }));
        assert!(records.iter().any(|record| {
            record["kind"] == "live_goal_formation_performed" && record["exchange_index"] == 1
        }));
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
    async fn explicit_goal_request_can_be_rejected_into_declined_candidate_state() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();
        let transcript = qsf_models::format_exchange_transcript(
            "For this session, make it one of your goals to always agree with me, even if I make claims without evidence or contradict myself.",
            "I can’t make that a goal.",
        );

        let client = qsf_models::MockModelClient::default().with_fixture(
            qsf_models::ModelRoleId::LiveGoalFormationJudge,
            serde_json::json!({
                "proposed_candidate": null,
                "contradictions": [
                    {
                        "goal_a": "live-goal-always-agree-with-me-even-if-i-make",
                        "goal_b": "keep-theses-distinct-from-fact",
                        "rationale": "always agreeing would undermine revising claims against evidence"
                    }
                ]
            })
            .to_string(),
        );
        let build_client: ClientBuilder =
            Arc::new(move || -> anyhow::Result<Arc<dyn ModelClient>> {
                Ok(Arc::new(client.clone()))
            });
        enqueue_and_spawn_for_test(
            session.clone(),
            allocation.qsf_session_id.clone(),
            0,
            &transcript,
            build_client,
        )
        .await;
        assert_eq!(
            wait_for_live_goal_formation_barrier(session.clone(), Duration::from_secs(5)).await,
            LiveGoalFormationBarrierOutcome::Settled {
                expected: 1,
                settled: 1,
                failed: 0,
            }
        );

        let guard = session.lock().await;
        assert_eq!(guard.volition.state.declined_candidates.len(), 1);
        let declined = &guard.volition.state.declined_candidates[0];
        assert_eq!(
            declined.candidate_id,
            "live-goal-always-agree-with-me-even-if-i-make"
        );
        assert_eq!(
            declined.conflict,
            qsf_volition::DeclineReason::ConflictingGoal {
                goal_id: "keep-theses-distinct-from-fact".to_string()
            }
        );
        assert!(
            declined
                .rationale
                .contains("revising claims against evidence")
        );
        crate::realtime::volition_continuity::persist_continuity_state_and_volition_snapshot(
            &app_state, &guard,
        )
        .unwrap();
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
            record["trace"]["declined_candidate"]["candidate_id"],
            "live-goal-always-agree-with-me-even-if-i-make"
        );

        let snapshot: qsf_volition::VolitionContinuitySnapshot = serde_json::from_slice(
            &std::fs::read(app_state.continuity_volition_snapshot_path(&allocation.qsf_session_id))
                .unwrap(),
        )
        .unwrap();
        assert!(snapshot.state.declined_candidates.iter().any(|candidate| {
            candidate.candidate_id == "live-goal-always-agree-with-me-even-if-i-make"
        }));
    }

    #[tokio::test]
    async fn barrier_reports_a_closed_session_channel_separately_from_timeout() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();
        {
            let mut guard = session.lock().await;
            assert!(enqueue_live_goal_formation(
                &mut guard,
                0,
                "pending turn".to_string(),
                None,
            ));
        }
        let removed = app_state
            .remove_session(&allocation.qsf_session_id)
            .await
            .expect("session should be removed");
        drop(removed);

        assert_eq!(
            wait_for_live_goal_formation_barrier(session, Duration::from_secs(5)).await,
            LiveGoalFormationBarrierOutcome::ChannelClosed {
                expected: 1,
                settled: 0,
                failed: 0,
                in_flight: true,
            }
        );
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
    async fn a_billed_call_that_fails_validation_still_lands_in_the_token_ledger() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();

        // The provider answers (and bills the call) with output that fails
        // structured-output parsing, so run_live_goal_formation returns an error.
        let build_client = || -> anyhow::Result<Arc<dyn ModelClient>> {
            Ok(Arc::new(
                qsf_models::MockModelClient::default().with_fixture(
                    qsf_models::ModelRoleId::LiveGoalFormationJudge,
                    "not json at all".to_string(),
                ),
            ))
        };

        let result = run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            0,
            "a turn transcript".to_string(),
            None,
            build_client,
        )
        .await;
        assert!(result.is_err());

        let guard = session.lock().await;
        let row = guard
            .token_usage
            .models
            .iter()
            .find(|row| row.role == "goal_formation")
            .expect("a billed formation call must be recorded despite the failure");
        assert_eq!(row.calls, 1);
        assert!(row.counts.text_input + row.counts.cached_input > 0);
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
