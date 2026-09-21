use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use qsf_models::{ModelRole, ModelRoleId};
use qsf_realtime_protocol::{
    OPENAI_REALTIME_VOICE_INPUT_TRANSCRIPTION_MODEL, OPENAI_REALTIME_VOICE_MODEL,
    OPENAI_REALTIME_WS_BASE_URL,
};
use time::OffsetDateTime;

use crate::cli::ProbeArgs;
use crate::realtime::live_goal_formation::{
    LiveGoalFormationBarrierOutcome, wait_for_live_goal_formation_barrier,
};
use crate::realtime::sideband::SidebandHandle;
use crate::realtime::sideband_attachment::SidebandAttachment;
use crate::realtime::volition_continuity::persist_continuity_state_and_volition_snapshot;
use crate::state::{AppState, SessionIdMode};

use super::{
    ModelIds, PhraseObservation, ProbeEvent, ProbeHeader, ProbeManifestMetadata, ProbeRunState,
    ProbeStatus, RuntimeCounters, SecretScanReport, SeedMode, TraceContractReport,
    WorldCorpusManifest, build_run_manifest, compare_phrase_expectation,
    compare_world_consultation_expectations, load_phrase_set, materialize_seed_bundle,
    parse_trace_contract, probe_verdict, reduce, render_formation_barrier, render_header,
    render_structured_partial_warning, render_turn, render_verdict, scan_for_secret, scan_run_dir,
    serialize_manifest, write_manifest_atomic,
};

#[derive(Clone, Debug)]
struct RunnerEnvironment {
    api_key: Result<String, String>,
    openai_base_url: String,
    realtime_ws_base_url: String,
}

impl RunnerEnvironment {
    fn from_process() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY")
                .map_err(|error| format!("OPENAI_API_KEY is required: {error}")),
            openai_base_url: crate::state::DEFAULT_OPENAI_BASE_URL.to_string(),
            realtime_ws_base_url: OPENAI_REALTIME_WS_BASE_URL.to_string(),
        }
    }
}

pub async fn run(args: ProbeArgs) -> anyhow::Result<()> {
    engine_logging::initialize();
    if let Some(seed_only) = &args.seed_only {
        materialize_seed_bundle(seed_only, OffsetDateTime::now_utc())?;
        println!("materialized warm seed bundle in {}", seed_only.display());
        return Ok(());
    }
    let phrases = load_phrase_set(&args.phrase_set)?;
    let phrase_hash = phrases.content_hash()?;
    let collision_root = collision_root(&args);
    let run_id = args
        .run_id
        .clone()
        .unwrap_or_else(|| generate_run_id(&collision_root));
    let run_dir = resolve_run_dir(&args, &run_id);
    ensure_fresh_run_dir(&run_dir)?;
    std::fs::create_dir_all(&run_dir).map_err(|error| {
        anyhow::anyhow!(
            "failed to create probe run directory {}: {error}",
            run_dir.display()
        )
    })?;
    let started_at = now_string();
    run_created_directory(
        &args,
        &phrases,
        &phrase_hash,
        &run_id,
        &run_dir,
        &started_at,
        RunnerEnvironment::from_process(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_created_directory(
    args: &ProbeArgs,
    phrases: &super::PhraseSet,
    phrase_hash: &str,
    run_id: &str,
    run_dir: &Path,
    started_at: &str,
    environment: RunnerEnvironment,
) -> anyhow::Result<()> {
    let seed_mode = if args.cold_start {
        SeedMode::ColdStart
    } else if let Err(error) = materialize_seed_bundle(run_dir, OffsetDateTime::now_utc()) {
        return finalize_without_session(
            args,
            phrases,
            phrase_hash,
            run_id,
            run_dir,
            started_at,
            default_metadata(SeedMode::WarmStartSeedBundleFailed),
            format!("failed to materialize warm seed bundle: {error}"),
            "",
        )
        .await;
    } else {
        SeedMode::WarmStartSeedBundle
    };
    let api_key = match environment.api_key {
        Ok(key) => key,
        Err(error) => {
            return finalize_without_session(
                args,
                phrases,
                phrase_hash,
                run_id,
                run_dir,
                started_at,
                default_metadata(seed_mode.clone()),
                error,
                "",
            )
            .await;
        }
    };
    let api_key_for_finalizer = api_key.clone();
    let state = match AppState::new_with_realtime_ws_base_url(
        api_key,
        environment.openai_base_url,
        environment.realtime_ws_base_url,
        run_dir,
        SessionIdMode::Default,
    ) {
        Ok(state) => state,
        Err(error) => {
            return finalize_without_session(
                args,
                phrases,
                phrase_hash,
                run_id,
                run_dir,
                started_at,
                default_metadata(seed_mode.clone()),
                error.to_string(),
                &api_key_for_finalizer,
            )
            .await;
        }
    };
    let allocation = match state.create_session().await {
        Ok(allocation) => allocation,
        Err(error) => {
            return finalize_with_session(
                args,
                phrases,
                phrase_hash,
                run_id,
                run_dir,
                started_at,
                state.clone(),
                None,
                metadata_from_state(seed_mode.clone(), &state, None),
                ProbeRunState::default(),
                Some(error.to_string()),
            )
            .await;
        }
    };
    let metadata = metadata_from_state(seed_mode, &state, Some(&allocation.session));
    let session_id = allocation.qsf_session_id;
    let session = state
        .session_runtime(&session_id)
        .await
        .expect("created session exists");
    let attachment = SidebandAttachment::ServerModelSession {
        model: allocation.session.model.clone(),
    };
    {
        let mut guard = session.lock().await;
        guard.sideband = Some(SidebandHandle::spawn(
            state.clone(),
            session_id.clone(),
            attachment,
        ));
    }
    println!(
        "{}",
        render_header(&ProbeHeader {
            run_id,
            state_dir: &run_dir.display().to_string(),
            phrases,
            hash: phrase_hash,
            model: &allocation.session.model,
            attachment_shape: &metadata.attachment_shape,
            reconnect_policy: &metadata.reconnect_policy,
            world_corpus: &metadata.world_corpus.state,
            seed_mode: seed_mode_label(&metadata.seed_mode),
        })
    );
    let (mut status_rx, mut completion_rx, inspection_rx) = {
        let guard = session.lock().await;
        (
            guard.subscribe_status(),
            guard.subscribe_trusted_turn_completion(),
            guard.subscribe_volition_inspection(),
        )
    };
    let mut run_state = ProbeRunState::default();
    let mut failure = None;
    match wait_for_attached(
        &mut status_rx,
        Duration::from_millis(args.attach_timeout_ms),
    )
    .await
    {
        AttachWait::Attached => reduce(&mut run_state, ProbeEvent::SidebandAttached),
        AttachWait::TimedOut => {
            reduce(&mut run_state, ProbeEvent::AttachTimedOut);
            failure = Some("attach timeout".to_string());
        }
        AttachWait::Terminated(reason) => {
            reduce(
                &mut run_state,
                ProbeEvent::SidebandTerminated {
                    reason: reason.clone(),
                },
            );
            failure = Some(format!("sideband terminated during attach: {reason}"));
        }
    }
    if failure.is_none() {
        for (phrase_index, phrase) in phrases.phrases.iter().enumerate() {
            reduce(
                &mut run_state,
                ProbeEvent::TurnSubmitted {
                    index: phrase_index,
                },
            );
            let started = Instant::now();
            let submit = {
                let guard = session.lock().await;
                guard
                    .sideband
                    .as_ref()
                    .expect("sideband set")
                    .submit_text_turn(phrase.text.clone())
            };
            if let Err(error) = submit {
                failure = Some(format!("submit turn {phrase_index}: {error}"));
                break;
            }
            match wait_for_completion(
                &mut completion_rx,
                &mut status_rx,
                phrase_index,
                Duration::from_millis(args.turn_timeout_ms),
            )
            .await
            {
                TurnWait::Completed(completion) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    let observation =
                        observe_phrase(&phrase.expected, completion.exchange_index, &inspection_rx);
                    reduce(
                        &mut run_state,
                        ProbeEvent::TurnCompleted {
                            index: completion.exchange_index,
                            promoted: completion.promoted,
                            elapsed_ms: elapsed,
                            inspection_captured: observation.inspection_captured,
                            arbitration_winner: observation.arbitration_winner.clone(),
                            expectation_differences: observation.differences,
                        },
                    );
                    println!(
                        "{}",
                        render_turn(
                            phrase_index,
                            phrases.phrases.len(),
                            &phrase.text,
                            elapsed,
                            completion.promoted,
                            observation.arbitration_winner.as_deref(),
                        )
                    );
                }
                TurnWait::TimedOut => {
                    reduce(
                        &mut run_state,
                        ProbeEvent::TurnTimedOut {
                            index: phrase_index,
                        },
                    );
                    failure = Some(format!("turn timeout at {phrase_index}"));
                    break;
                }
                TurnWait::Terminated(reason) => {
                    reduce(
                        &mut run_state,
                        ProbeEvent::SidebandTerminated {
                            reason: reason.clone(),
                        },
                    );
                    failure = Some(format!("sideband terminated: {reason}"));
                    break;
                }
            }
            if phrase_index + 1 < phrases.phrases.len() {
                tokio::time::sleep(Duration::from_millis(args.turn_delay_ms)).await;
            }
        }
    }
    finalize_with_session(
        args,
        phrases,
        phrase_hash,
        run_id,
        run_dir,
        started_at,
        state,
        Some(session_id),
        metadata,
        run_state,
        failure,
    )
    .await
}

fn observe_phrase(
    expected: &super::PhraseExpected,
    exchange_index: usize,
    inspection_rx: &tokio::sync::watch::Receiver<
        Option<crate::realtime::volition_inspection_capture::VolitionInspectionCapture>,
    >,
) -> PhraseObservation {
    let inspection = inspection_rx.borrow().clone();
    let inspection = inspection
        .as_ref()
        .filter(|capture| capture.exchange_index == exchange_index);
    compare_phrase_expectation(expected, inspection)
}

#[allow(clippy::too_many_arguments)]
async fn finalize_without_session(
    args: &ProbeArgs,
    phrases: &super::PhraseSet,
    phrase_hash: &str,
    run_id: &str,
    run_dir: &Path,
    started_at: &str,
    metadata: ProbeManifestMetadata,
    original_failure: String,
    secret: &str,
) -> anyhow::Result<()> {
    let counters = RuntimeCounters {
        phrase_count: phrases.phrases.len(),
        ..RuntimeCounters::default()
    };
    let mut finalization_errors = Vec::new();
    let secrets = match scan_run_dir(run_dir, secret) {
        Ok(report) => report,
        Err(error) => {
            finalization_errors.push(format!("secret scan under {}: {error}", run_dir.display()));
            SecretScanReport::default()
        }
    };
    finalize_manifest(
        args,
        phrases,
        phrase_hash,
        run_id,
        run_dir,
        started_at,
        metadata,
        ProbeRunState::default(),
        counters,
        parse_trace_contract(b"", &[]),
        secrets,
        Some(original_failure),
        finalization_errors,
        secret,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finalize_with_session(
    args: &ProbeArgs,
    phrases: &super::PhraseSet,
    phrase_hash: &str,
    run_id: &str,
    run_dir: &Path,
    started_at: &str,
    state: AppState,
    session_id: Option<String>,
    metadata: ProbeManifestMetadata,
    mut run_state: ProbeRunState,
    original_failure: Option<String>,
) -> anyhow::Result<()> {
    let mut finalization_errors = Vec::new();
    let mut counters = RuntimeCounters {
        phrase_count: phrases.phrases.len(),
        session_created: session_id.is_some(),
        ..RuntimeCounters::default()
    };
    let mut diagnostics_path = None;
    if let Some(session_id) = session_id {
        if let Some(session) = state.session_runtime(&session_id).await {
            match wait_for_live_goal_formation_barrier(
                session.clone(),
                Duration::from_millis(args.formation_timeout_ms),
            )
            .await
            {
                LiveGoalFormationBarrierOutcome::Settled {
                    expected,
                    settled,
                    failed,
                } => reduce(
                    &mut run_state,
                    ProbeEvent::FormationBarrierSettled {
                        expected,
                        settled,
                        failed,
                        timeout_ms: args.formation_timeout_ms,
                    },
                ),
                LiveGoalFormationBarrierOutcome::TimedOut {
                    expected,
                    settled,
                    failed,
                    ..
                }
                | LiveGoalFormationBarrierOutcome::ChannelClosed {
                    expected,
                    settled,
                    failed,
                    ..
                } => reduce(
                    &mut run_state,
                    ProbeEvent::FormationBarrierTimedOut {
                        expected,
                        settled,
                        failed,
                        timeout_ms: args.formation_timeout_ms,
                    },
                ),
            }
            {
                let guard = session.lock().await;
                if let Err(error) = persist_continuity_state_and_volition_snapshot(&state, &guard) {
                    finalization_errors.push(format!("persist continuity: {error}"));
                }
                counters.promoted_turn_count = guard.trusted_promoted_exchange_count;
                counters.promoted_exchange_indices = run_state.promoted_exchange_indices();
                counters.non_promotable_exchange_indices = guard
                    .non_promotable_exchange_indices
                    .iter()
                    .copied()
                    .collect();
                counters.non_promotable_exchange_indices.sort_unstable();
                counters.degradation_epoch = guard.degradation_epoch();
                counters.degradation_reasons = guard.degradation_reasons().to_vec();
                counters.terminated_reason =
                    guard.sideband_termination_reason().map(ToOwned::to_owned);
                counters.output_audio_delta_count = guard.session_output_audio_delta_count;
                counters.output_audio_delta_byte_count =
                    guard.session_output_audio_delta_byte_count;
                counters.token_ledger = guard.token_usage.clone();
                diagnostics_path = Some(guard.diagnostics.path().to_path_buf());
            }
            if let Err(error) =
                crate::realtime::session_lifecycle::stop_session(&state, session_id).await
            {
                finalization_errors.push(format!("stop and join sideband: {error}"));
            }
        }
    }
    println!("{}", render_formation_barrier(run_state.formation.as_ref()));
    let diagnostics = match diagnostics_path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("session diagnostics path was not captured"))
        .and_then(|path| {
            std::fs::read(path)
                .map_err(anyhow::Error::from)
                .map(|bytes| (path, bytes))
        }) {
        Ok((_path, bytes)) => Some(bytes),
        Err(error) => {
            finalization_errors.push(format!("read diagnostics: {error}"));
            None
        }
    };
    let traces = diagnostics.as_deref().map_or_else(
        || TraceContractReport {
            complete: false,
            turns: vec![],
            parse_errors: vec!["diagnostics unavailable".to_string()],
        },
        |bytes| parse_trace_contract(bytes, &counters.promoted_exchange_indices),
    );
    if let Some(diagnostics) = diagnostics.as_deref() {
        for difference in compare_world_consultation_expectations(phrases, diagnostics) {
            reduce(
                &mut run_state,
                ProbeEvent::ExpectationDifferencesObserved {
                    index: difference.phrase_index,
                    differences: difference.differences,
                },
            );
        }
    }
    reduce(&mut run_state, ProbeEvent::RunFinished);
    let secret = state.openai_api_key();
    let secrets = match scan_run_dir(run_dir, secret) {
        Ok(report) => report,
        Err(error) => {
            finalization_errors.push(format!("secret scan under {}: {error}", run_dir.display()));
            SecretScanReport::default()
        }
    };
    finalize_manifest(
        args,
        phrases,
        phrase_hash,
        run_id,
        run_dir,
        started_at,
        metadata,
        run_state,
        counters,
        traces,
        secrets,
        original_failure,
        finalization_errors,
        secret,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finalize_manifest(
    args: &ProbeArgs,
    phrases: &super::PhraseSet,
    phrase_hash: &str,
    run_id: &str,
    run_dir: &Path,
    started_at: &str,
    metadata: ProbeManifestMetadata,
    run_state: ProbeRunState,
    counters: RuntimeCounters,
    traces: TraceContractReport,
    mut secrets: SecretScanReport,
    original_failure: Option<String>,
    finalization_errors: Vec<String>,
    secret: &str,
) -> anyhow::Result<()> {
    let build = |secrets: SecretScanReport| {
        let verdict = probe_verdict(
            &run_state,
            &counters,
            &traces,
            &secrets,
            &finalization_errors,
            original_failure.as_deref(),
        );
        let manifest = build_run_manifest(
            run_id.to_string(),
            phrases.id.clone(),
            phrase_hash.to_string(),
            run_dir.display().to_string(),
            args.git_commit.clone(),
            started_at.to_string(),
            now_string(),
            metadata.clone(),
            run_state.clone(),
            counters.clone(),
            traces.clone(),
            secrets,
            verdict.clone(),
            original_failure.clone(),
            finalization_errors.clone(),
        );
        (verdict, manifest)
    };
    let mut candidate = build(secrets.clone());
    loop {
        let serialized = serialize_manifest(&candidate.1).map_err(|error| {
            let cause = original_failure
                .as_deref()
                .unwrap_or("probe finalization failed");
            anyhow::anyhow!("{cause}; terminal manifest serialization failed: {error}")
        })?;
        let manifest_path_recorded = secrets.paths.iter().any(|path| path == "run-manifest.json");
        if scan_for_secret(&serialized, secret) && !manifest_path_recorded {
            secrets.found = true;
            secrets.paths.push("run-manifest.json".to_string());
            candidate = build(secrets.clone());
            continue;
        }
        break;
    }
    let (verdict, manifest) = candidate;
    let manifest_path = run_dir.join("run-manifest.json");
    if let Err(error) = write_manifest_atomic(&manifest_path, &manifest) {
        let cause = original_failure
            .as_deref()
            .unwrap_or("probe finalization failed");
        log::error!(
            "probe `{run_id}` could not write terminal manifest `{}`: {error}; original cause: {cause}; verdict: {}",
            manifest_path.display(),
            render_verdict(&verdict)
        );
        return Err(anyhow::anyhow!(
            "{cause}; terminal manifest `{}` could not be written: {error}",
            manifest_path.display()
        ));
    }
    for clause in &verdict.structured_clauses {
        println!("{}", render_structured_partial_warning(clause));
    }
    println!("{}", render_verdict(&verdict));
    if let Some(original_failure) = original_failure {
        return Err(anyhow::anyhow!(
            "{original_failure}; probe verdict is {:?}",
            verdict.status
        ));
    }
    if !matches!(verdict.status, ProbeStatus::Passed) {
        anyhow::bail!("probe verdict is {:?}", verdict.status);
    }
    Ok(())
}

fn default_metadata(seed_mode: SeedMode) -> ProbeManifestMetadata {
    ProbeManifestMetadata {
        attachment_shape: "server_model_session".to_string(),
        reconnect_policy: "fail_closed_after_first_attach".to_string(),
        model_ids: ModelIds {
            realtime_voice: OPENAI_REALTIME_VOICE_MODEL.to_string(),
            input_transcription: Some(OPENAI_REALTIME_VOICE_INPUT_TRANSCRIPTION_MODEL.to_string()),
            live_goal_formation: ModelRole::predefined(ModelRoleId::LiveGoalFormationJudge)
                .default_model,
        },
        world_corpus: WorldCorpusManifest {
            state: "not_loaded".to_string(),
            marker: None,
            detail: Some("application state was not initialized".to_string()),
            resolution_source: None,
            degradation_reason: None,
        },
        seed_mode,
    }
}

fn metadata_from_state(
    seed_mode: SeedMode,
    state: &AppState,
    session: Option<&crate::state::BrowserSessionConfig>,
) -> ProbeManifestMetadata {
    let mut metadata = default_metadata(seed_mode);
    if let Some(session) = session {
        metadata.model_ids.realtime_voice = session.model.clone();
        metadata.model_ids.input_transcription = session.input_transcription_model.clone();
    }
    metadata.world_corpus = world_corpus_manifest(state.world_corpus());
    metadata
}

fn world_corpus_manifest(
    world_corpus: &crate::realtime::world_consultation::WorldCorpus,
) -> WorldCorpusManifest {
    match world_corpus {
        crate::realtime::world_consultation::WorldCorpus::Ready(corpus) => WorldCorpusManifest {
            state: "ready".to_string(),
            marker: serde_json::to_value(&corpus.marker).ok(),
            detail: Some(format!(
                "{} articles at {}; schema drift {:?}",
                corpus.articles_indexed,
                corpus.corpus_path.display(),
                corpus.schema_drift
            )),
            resolution_source: Some(corpus.resolution_source.to_string()),
            degradation_reason: corpus.degraded_reason.clone(),
        },
        crate::realtime::world_consultation::WorldCorpus::Unavailable { reason } => {
            WorldCorpusManifest {
                state: "unavailable".to_string(),
                marker: None,
                detail: Some(reason.clone()),
                resolution_source: None,
                degradation_reason: None,
            }
        }
    }
}

fn seed_mode_label(seed_mode: &SeedMode) -> &'static str {
    match seed_mode {
        SeedMode::ColdStart => "cold-start",
        SeedMode::WarmStartSeedBundle => "warm-start-seed-bundle",
        SeedMode::WarmStartSeedBundleFailed => "warm-start-seed-bundle-failed",
    }
}

enum AttachWait {
    Attached,
    TimedOut,
    Terminated(String),
}

enum TurnWait {
    Completed(crate::state::TrustedTurnCompletion),
    TimedOut,
    Terminated(String),
}

async fn wait_for_attached(
    status: &mut tokio::sync::watch::Receiver<crate::state::SidebandStatus>,
    timeout: Duration,
) -> AttachWait {
    if status.borrow().attached {
        return AttachWait::Attached;
    }
    if let Some(reason) = status.borrow().terminated.clone() {
        return AttachWait::Terminated(reason);
    }
    let wait = async {
        loop {
            if status.changed().await.is_err() {
                return AttachWait::Terminated("sideband status channel closed".to_string());
            }
            if status.borrow().attached {
                return AttachWait::Attached;
            }
            if let Some(reason) = status.borrow().terminated.clone() {
                return AttachWait::Terminated(reason);
            }
        }
    };
    tokio::time::timeout(timeout, wait)
        .await
        .unwrap_or(AttachWait::TimedOut)
}

async fn wait_for_completion(
    completion: &mut tokio::sync::watch::Receiver<Option<crate::state::TrustedTurnCompletion>>,
    status: &mut tokio::sync::watch::Receiver<crate::state::SidebandStatus>,
    index: usize,
    timeout: Duration,
) -> TurnWait {
    let wait = async {
        loop {
            tokio::select! {
                changed = completion.changed() => {
                    if changed.is_err() {
                        return TurnWait::Terminated("trusted completion channel closed".to_string());
                    }
                    if let Some(value) = completion.borrow().as_ref()
                        && value.exchange_index == index
                    {
                        return TurnWait::Completed(value.clone());
                    }
                }
                changed = status.changed() => {
                    if changed.is_err() {
                        return TurnWait::Terminated("sideband status channel closed".to_string());
                    }
                    if let Some(reason) = status.borrow().terminated.clone() {
                        return TurnWait::Terminated(reason);
                    }
                }
            }
        }
    };
    tokio::time::timeout(timeout, wait)
        .await
        .unwrap_or(TurnWait::TimedOut)
}

fn resolve_run_dir(args: &ProbeArgs, run_id: &str) -> PathBuf {
    args.state_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("state").join("probe").join(run_id))
}

fn collision_root(args: &ProbeArgs) -> PathBuf {
    args.state_dir
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("state").join("probe"))
}

fn ensure_fresh_run_dir(run_dir: &Path) -> anyhow::Result<()> {
    let diagnostics = run_dir.join("diagnostics");
    let manifest = run_dir.join("run-manifest.json");
    if diagnostics.exists() || manifest.exists() {
        anyhow::bail!(
            "probe run directory `{}` is not fresh: remove or choose another directory; existing diagnostics or terminal manifests cannot be reused",
            run_dir.display()
        );
    }
    Ok(())
}

fn generate_run_id(root: &Path) -> String {
    let base = OffsetDateTime::now_utc()
        .format(&time::macros::format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .unwrap_or_else(|_| "probe".to_string());
    let mut id = base.clone();
    let mut suffix = 2;
    while root.join(&id).exists() {
        id = format!("{base}-{suffix}");
        suffix += 1;
    }
    id
}

fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use futures_util::{SinkExt, StreamExt};
    use tempfile::TempDir;
    use tokio::io::AsyncReadExt;
    use tokio::sync::oneshot;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    use super::*;

    fn probe_args(run_dir: &Path) -> ProbeArgs {
        ProbeArgs {
            phrase_set: "smoke".to_string(),
            state_dir: Some(run_dir.to_path_buf()),
            run_id: Some("test-run".to_string()),
            cold_start: true,
            turn_delay_ms: 0,
            turn_timeout_ms: 30,
            attach_timeout_ms: 30,
            formation_timeout_ms: 30,
            git_commit: None,
            seed_only: None,
        }
    }

    fn test_environment(websocket_url: String) -> RunnerEnvironment {
        RunnerEnvironment {
            api_key: Ok("test-api-key".to_string()),
            openai_base_url: "http://127.0.0.1:9".to_string(),
            realtime_ws_base_url: websocket_url,
        }
    }

    async fn run_test_probe(
        args: &ProbeArgs,
        environment: RunnerEnvironment,
    ) -> anyhow::Result<()> {
        let phrases = load_phrase_set("smoke").expect("smoke");
        let hash = phrases.content_hash().expect("hash");
        run_created_directory(
            args,
            &phrases,
            &hash,
            "test-run",
            args.state_dir.as_deref().expect("state dir"),
            "2026-07-31T00:00:00Z",
            environment,
        )
        .await
    }

    fn read_manifest(run_dir: &Path) -> super::super::RunManifest {
        serde_json::from_slice(
            &std::fs::read(run_dir.join("run-manifest.json")).expect("manifest bytes"),
        )
        .expect("parseable manifest")
    }

    async fn spawn_rejecting_stub() -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let count = Arc::new(AtomicUsize::new(0));
        let task_count = count.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                task_count.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer).await;
            }
        });
        (format!("ws://{address}/v1/realtime"), count, task)
    }

    async fn spawn_attached_noncompleting_stub()
    -> (String, oneshot::Receiver<()>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let (closed_tx, closed_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection");
            let mut websocket = accept_async(stream).await.expect("websocket");
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "session.updated",
                        "event_id": "stub-attached"
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("session updated");
            while let Some(message) = websocket.next().await {
                match message {
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            let _ = closed_tx.send(());
        });
        (format!("ws://{address}/v1/realtime"), closed_rx, task)
    }

    #[tokio::test]
    async fn attach_timeout_writes_failed_manifest_and_stops_retrying_sideband() {
        let tempdir = TempDir::new().expect("tempdir");
        let run_dir = tempdir.path().join("run");
        std::fs::create_dir_all(&run_dir).expect("run dir");
        let (url, count, stub) = spawn_rejecting_stub().await;
        let args = probe_args(&run_dir);

        let error = run_test_probe(&args, test_environment(url))
            .await
            .expect_err("attach timeout");

        assert!(error.to_string().contains("attach timeout"));
        let manifest = read_manifest(&run_dir);
        assert_eq!(manifest.status, ProbeStatus::Failed);
        assert!(
            manifest
                .failing_clauses
                .contains(&"attach_timeout".to_string())
        );
        let after_return = count.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(count.load(Ordering::SeqCst), after_return);
        stub.abort();
    }

    #[tokio::test]
    async fn turn_timeout_writes_failed_manifest_and_joins_sideband() {
        let tempdir = TempDir::new().expect("tempdir");
        let run_dir = tempdir.path().join("run");
        std::fs::create_dir_all(&run_dir).expect("run dir");
        let (url, closed, stub) = spawn_attached_noncompleting_stub().await;
        let args = probe_args(&run_dir);

        let error = run_test_probe(&args, test_environment(url))
            .await
            .expect_err("turn timeout");

        assert!(error.to_string().contains("turn timeout at 0"));
        let manifest = read_manifest(&run_dir);
        assert_eq!(manifest.status, ProbeStatus::Failed);
        assert!(
            manifest
                .failing_clauses
                .contains(&"turn_timeout:0".to_string())
        );
        tokio::time::timeout(Duration::from_secs(1), closed)
            .await
            .expect("sideband close observed")
            .expect("close signal");
        stub.await.expect("stub joined");
    }

    #[tokio::test]
    async fn unwritable_manifest_target_retains_original_failure() {
        let tempdir = TempDir::new().expect("tempdir");
        let run_dir = tempdir.path().join("run");
        std::fs::create_dir_all(run_dir.join(".run-manifest.json.tmp"))
            .expect("block temporary manifest");
        let (url, _count, stub) = spawn_rejecting_stub().await;
        let args = probe_args(&run_dir);

        let error = run_test_probe(&args, test_environment(url))
            .await
            .expect_err("manifest write fails")
            .to_string();

        assert!(error.contains("attach timeout"));
        assert!(error.contains("could not be written"));
        stub.abort();
    }

    #[tokio::test]
    async fn pre_session_failure_is_recorded_as_original_infrastructure_cause() {
        let tempdir = TempDir::new().expect("tempdir");
        let run_dir = tempdir.path().join("run");
        std::fs::create_dir_all(&run_dir).expect("run dir");
        let args = probe_args(&run_dir);
        let environment = RunnerEnvironment {
            api_key: Err("OPENAI_API_KEY is required: not present".to_string()),
            openai_base_url: "unused".to_string(),
            realtime_ws_base_url: "unused".to_string(),
        };

        let error = run_test_probe(&args, environment)
            .await
            .expect_err("pre-session failure");

        assert!(error.to_string().contains("OPENAI_API_KEY is required"));
        let manifest = read_manifest(&run_dir);
        assert_eq!(manifest.status, ProbeStatus::InfrastructureError);
        assert_eq!(
            manifest.original_failure.as_deref(),
            Some("OPENAI_API_KEY is required: not present")
        );
        assert!(manifest.finalization_errors.is_empty());
    }

    #[tokio::test]
    async fn failed_warm_seed_materialization_is_recorded_as_failed_provenance() {
        let tempdir = TempDir::new().expect("tempdir");
        let run_dir = tempdir.path().join("run");
        std::fs::create_dir_all(&run_dir).expect("run dir");
        std::fs::write(run_dir.join("continuity"), "blocks seed directory").expect("blocking file");
        let mut args = probe_args(&run_dir);
        args.cold_start = false;

        let error = run_test_probe(&args, test_environment("unused".to_string()))
            .await
            .expect_err("seed failure");

        assert!(
            error
                .to_string()
                .contains("failed to materialize warm seed bundle")
        );
        let manifest = read_manifest(&run_dir);
        assert_eq!(manifest.seed_mode, SeedMode::WarmStartSeedBundleFailed);
    }

    #[test]
    fn existing_probe_artifacts_make_run_directory_non_fresh() {
        let tempdir = TempDir::new().expect("tempdir");
        std::fs::create_dir_all(tempdir.path().join("diagnostics")).expect("diagnostics");

        let error = ensure_fresh_run_dir(tempdir.path())
            .expect_err("stale directory")
            .to_string();

        assert!(error.contains("is not fresh"));
    }

    #[test]
    fn collision_probe_uses_supplied_run_directory_parent() {
        let tempdir = TempDir::new().expect("tempdir");
        let supplied = tempdir.path().join("custom").join("run");
        let args = probe_args(&supplied);

        assert_eq!(collision_root(&args), tempdir.path().join("custom"));
    }

    #[tokio::test]
    async fn seed_only_materializes_the_shared_warm_start_bundle_without_credentials() {
        let destination = TempDir::new().expect("destination");
        let mut args = probe_args(destination.path());
        args.seed_only = Some(destination.path().to_path_buf());

        run(args).await.expect("seed only");

        let continuity = destination.path().join("continuity/default");
        assert!(continuity.join("memory-store.json").is_file());
        assert!(continuity.join("volition-state.json").is_file());
        assert!(continuity.join("continuity-manifest.json").is_file());
        assert!(!continuity.join("session-state.json").exists());
    }

    #[test]
    fn manifest_records_bundled_fallback_provenance_after_bad_configuration() {
        use std::sync::Arc;

        use qsf_corpus::{CorpusIndex, CorpusMarker, CorpusSchemaDrift, resolve_corpus_path};

        let resolution =
            resolve_corpus_path(Some(std::path::PathBuf::from("definitely-missing-corpus")));

        let corpus = crate::realtime::world_consultation::WorldCorpus::Ready(
            crate::realtime::world_consultation::ReadyWorldCorpus {
                index: Arc::new(CorpusIndex::new(vec![])),
                marker: CorpusMarker {
                    schema_version: 1,
                    producer: "test".to_string(),
                    article_patterns: vec![],
                    generated_artifacts: vec![],
                    internal_state: vec![],
                },
                schema_drift: CorpusSchemaDrift::None,
                articles_indexed: 0,
                corpus_path: resolution.corpus_path,
                resolution_source: resolution.source,
                degraded_reason: resolution.degraded_reason,
            },
        );

        let manifest = world_corpus_manifest(&corpus);

        assert_eq!(manifest.state, "ready");
        assert_eq!(
            manifest.resolution_source.as_deref(),
            Some("bundled_fixture_after_missing_configured_path")
        );
        assert_eq!(
            manifest.degradation_reason.as_deref(),
            Some("configured world corpus path is unavailable: definitely-missing-corpus")
        );
    }
}
