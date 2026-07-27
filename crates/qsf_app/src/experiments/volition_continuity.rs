use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use qsf_models::{
    CoherenceJudge, LiveGoalFormationJudge, ModelBackedCoherenceJudge,
    ModelBackedLiveGoalFormationJudge, ModelClient, coherence_judge_goal_set,
    live_goal_formation_stable_prefix_hash,
};
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::session::{
    ContinuityManifest, StateDirectoryResolution, resolve_shared_state_directory_from_env,
};
use qsf_diagnostics::{DiagnosticRecord, REALTIME_BOUNDED_INITIATIVE_KIND, decode_envelope};
use qsf_volition::{
    AdmissionResolution, DeclinedCandidate, REALTIME_SEED_FIXTURE_ID, ReviewedVolitionSeed,
    VolitionConsolidationReport, VolitionConsolidationSnapshotRecord, VolitionContinuitySnapshot,
    VolitionTurnOutcome, apply, build_state_inspection, build_volition_consolidation_report,
    effective_tier_from_tension_ids, fixture_for_seed_id, load_reviewed_volition_seed,
    newly_declined_candidate, persist_volition_continuity_snapshot, resolve_formed_candidate,
    resolve_sweep,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::volition_trace_support::{
    goal_set_snapshot_json, goal_status_diff, write_coherence_trace, write_volition_trace,
};

const CONTINUITY_SESSION_ID_ENV_VAR: &str = "QSF_VOLITION_CONTINUITY_SESSION_ID";
const CONTINUITY_REPORT_JSON: &str = "volition-continuity-report.json";
const CONTINUITY_REPORT_MARKDOWN: &str = "volition-continuity-report.md";

pub struct VolitionContinuityExperiment;

impl Experiment for VolitionContinuityExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionContinuity
    }

    fn description(&self) -> &'static str {
        "Read realtime volition continuity artifacts, consolidate them, and emit a reviewable report"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let resolution = resolve_shared_state_directory_from_env();
        let session_id =
            std::env::var(CONTINUITY_SESSION_ID_ENV_VAR).unwrap_or_else(|_| "default".to_string());
        self.run_with_state_resolution(context, resolution, &session_id)
    }
}

impl VolitionContinuityExperiment {
    fn run_with_state_resolution(
        &self,
        context: &mut RunContext,
        state_resolution: StateDirectoryResolution,
        qsf_session_id: &str,
    ) -> anyhow::Result<ExperimentOutcome> {
        let continuity_dir = state_resolution
            .persist_state_dir
            .join("continuity")
            .join(qsf_session_id);
        let diagnostics_path = state_resolution
            .persist_state_dir
            .join("diagnostics")
            .join(format!("{qsf_session_id}.jsonl"));
        let manifest_path = continuity_dir.join("continuity-manifest.json");
        let snapshot_path = snapshot_path_from_manifest(&manifest_path, &continuity_dir)?;

        context.record_event(
            EventType::InputReceived,
            serde_json::json!({
                "state_root": state_resolution.persist_state_dir.display().to_string(),
                "continuity_dir": continuity_dir.display().to_string(),
                "diagnostics_path": diagnostics_path.display().to_string(),
                "snapshot_path": snapshot_path.display().to_string(),
                "reviewed_seed_path": continuity_dir.join("volition-seed.reviewed.json").display().to_string(),
                "session_id": qsf_session_id,
            }),
            None,
        )?;

        let started_at = Instant::now();
        let report = consolidate_session_volition(
            &state_resolution.persist_state_dir,
            qsf_session_id,
        )?
        .with_context(|| {
            format!(
                "no volition continuity snapshot found for session `{qsf_session_id}` in `{}`",
                continuity_dir.display()
            )
        })?;
        let elapsed_ns = elapsed_ns(started_at);

        let report_json = serde_json::to_string_pretty(&report)
            .context("failed to serialize volition continuity report")?;
        fs::write(context.run_dir().join(CONTINUITY_REPORT_JSON), &report_json)
            .with_context(|| "failed to write volition continuity report json".to_string())?;
        fs::write(
            context.run_dir().join(CONTINUITY_REPORT_MARKDOWN),
            render_markdown_report(&report),
        )
        .with_context(|| "failed to write volition continuity report markdown".to_string())?;

        let trace = TraceRecord::new(
            context.experiment_id(),
            "volition-continuity-consolidation",
            format!(
                "session={qsf_session_id} snapshot={}",
                snapshot_path.display()
            ),
            format!("items={}", report.items.len()),
        )
        .with_latency_ns(elapsed_ns)
        .with_details(serde_json::json!({
            "report": &report,
            "snapshot_path": snapshot_path.display().to_string(),
            "diagnostics_path": diagnostics_path.display().to_string(),
        }));
        let trace_id = trace.trace_id;
        context.record_trace(trace)?;

        context.record_event(
            EventType::OutputProduced,
            serde_json::json!({
                "report_json": CONTINUITY_REPORT_JSON,
                "report_markdown": CONTINUITY_REPORT_MARKDOWN,
                "item_count": report.items.len(),
            }),
            Some(trace_id),
        )?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Consolidated {} volition continuity item{} for session `{}` into reviewable report artifacts.",
                report.items.len(),
                if report.items.len() == 1 { "" } else { "s" },
                qsf_session_id
            ),
            observations: vec![
                "The consolidation report cites artifact references from the persisted snapshot and diagnostics stream.".to_string(),
                "Unsurfaced initiatives are grounded in the durable initiative trace fields, not inferred from record presence.".to_string(),
            ],
            failure_modes: vec![
                format!(
                    "The continuity snapshot `{}` must exist and parse as a volition continuity snapshot.",
                    snapshot_path.display()
                ),
                format!(
                    "The diagnostics stream `{}` must contain parseable realtime bounded-initiative traces when present.",
                    diagnostics_path.display()
                ),
            ],
            follow_up_questions: vec![
                "Should the report later include direct links to the reviewed seed artifact?".to_string(),
            ],
            decision_candidates: vec![
                "Keep volition continuity consolidation reviewable and artifact-grounded rather than implicitly mutating seed state.".to_string(),
            ],
            extra_artifacts: vec![
                CONTINUITY_REPORT_JSON.to_string(),
                CONTINUITY_REPORT_MARKDOWN.to_string(),
            ],
        })
    }
}

/// Resolve the volition snapshot path for a session, defaulting to `volition-state.json`
/// if the manifest does not record one.
pub(crate) fn snapshot_path_from_manifest(
    manifest_path: &Path,
    continuity_dir: &Path,
) -> anyhow::Result<PathBuf> {
    let manifest = ContinuityManifest::load_or_default(manifest_path)?;
    let relative = manifest
        .current_volition_snapshot_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("volition-state.json"));
    Ok(if relative.is_absolute() {
        relative
    } else {
        continuity_dir.join(relative)
    })
}

struct RealtimeContinuityPaths {
    continuity_dir: PathBuf,
    diagnostics_path: PathBuf,
}

fn realtime_continuity_paths(
    state_root_or_continuity_dir: &Path,
    session_id: &str,
) -> RealtimeContinuityPaths {
    if state_root_or_continuity_dir
        .join("continuity-manifest.json")
        .exists()
    {
        let state_root = state_root_or_continuity_dir
            .parent()
            .and_then(|continuity_root| {
                (continuity_root.file_name().and_then(|name| name.to_str()) == Some("continuity"))
                    .then(|| continuity_root.parent())
                    .flatten()
            })
            .unwrap_or(state_root_or_continuity_dir);
        return RealtimeContinuityPaths {
            continuity_dir: state_root_or_continuity_dir.to_path_buf(),
            diagnostics_path: state_root
                .join("diagnostics")
                .join(format!("{session_id}.jsonl")),
        };
    }

    RealtimeContinuityPaths {
        continuity_dir: state_root_or_continuity_dir
            .join("continuity")
            .join(session_id),
        diagnostics_path: state_root_or_continuity_dir
            .join("diagnostics")
            .join(format!("{session_id}.jsonl")),
    }
}

/// Read the realtime continuity artifacts for `session_id` from either the realtime state root or
/// the already-resolved per-session continuity directory and run pure consolidation. Returns
/// `None` when no continuity snapshot exists for that session (i.e. no realtime volition
/// continuity has been persisted yet).
///
/// Called both from `VolitionContinuityExperiment` and from the sleep-pass
/// `commit_cross_session_sleep` flow so consolidation runs through the standard sleep path.
pub(crate) fn consolidate_session_volition(
    state_root_or_continuity_dir: &Path,
    session_id: &str,
) -> anyhow::Result<Option<VolitionConsolidationReport>> {
    let paths = realtime_continuity_paths(state_root_or_continuity_dir, session_id);
    let continuity_dir = paths.continuity_dir;
    let manifest_path = continuity_dir.join("continuity-manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let snapshot_path = snapshot_path_from_manifest(&manifest_path, &continuity_dir)?;
    if !snapshot_path.exists() {
        return Ok(None);
    }

    let snapshot =
        VolitionContinuitySnapshot::load_or_upgrade(&snapshot_path).with_context(|| {
            format!(
                "failed to load volition snapshot `{}`",
                snapshot_path.display()
            )
        })?;
    let snapshot_record = VolitionConsolidationSnapshotRecord {
        artifact_reference: snapshot_path.display().to_string(),
        snapshot,
    };

    let initiative_outcomes = load_initiative_outcomes(&paths.diagnostics_path)?;

    let reviewed_seed_path = continuity_dir.join("volition-seed.reviewed.json");
    let reviewed_seed = if reviewed_seed_path.exists() {
        Some(load_reviewed_seed(&reviewed_seed_path)?)
    } else {
        None
    };

    let report = build_volition_consolidation_report(
        session_id,
        REALTIME_SEED_FIXTURE_ID,
        &[snapshot_record],
        &initiative_outcomes,
        reviewed_seed.as_ref(),
    );

    Ok(Some(report))
}

/// Summary of what the sleep goal-maintenance pass changed, for the sleep outcome observations.
pub(crate) struct SleepVolitionMaintenanceSummary {
    pub admitted_goal_id: Option<String>,
    pub declined_candidate: Option<DeclinedCandidate>,
    pub swept_goal_ids: Vec<String>,
    /// Path of the volition continuity snapshot re-persisted by this maintenance pass, so the
    /// sleep change view can list it among the state files written. `Some` on every path that
    /// reaches the snapshot re-persist below; the early `None` returns never build a summary.
    pub persisted_snapshot_path: Option<PathBuf>,
}

/// The real sleep/consolidation goal-maintenance pass (whole-history formation + whole-set
/// sweep), the live analogue of the offline `live-goal-formation-and-coherence` harness's sleep
/// steps. Loads the persisted volition snapshot, runs one whole-history formation call and one
/// whole-set coherence sweep through the **same** pure resolvers the live per-turn hook uses
/// (`resolve_formed_candidate` / `resolve_sweep`), applies the resulting events, re-persists the
/// snapshot, and records a `live-goal-formation` and a `goal-coherence-check` trace per the
/// trace-completeness contract. Returns `None` when no continuity snapshot exists for the session
/// or its seed fixture is not reconstructable.
///
/// The default mock client forms no candidate and detects no contradictions, so the default path
/// runs end to end (persisting an unchanged snapshot and two trace records) without a real model.
pub(crate) fn run_sleep_volition_goal_maintenance(
    context: &mut RunContext,
    client: &dyn ModelClient,
    state_root_or_continuity_dir: &Path,
    session_id: &str,
    whole_history_input: &str,
) -> anyhow::Result<Option<SleepVolitionMaintenanceSummary>> {
    let paths = realtime_continuity_paths(state_root_or_continuity_dir, session_id);
    let continuity_dir = paths.continuity_dir;
    let manifest_path = continuity_dir.join("continuity-manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let snapshot_path = snapshot_path_from_manifest(&manifest_path, &continuity_dir)?;
    if !snapshot_path.exists() {
        return Ok(None);
    }

    let snapshot = VolitionContinuitySnapshot::load_or_upgrade(&snapshot_path)?;
    let Some(fixture) = fixture_for_seed_id(&snapshot.seed_fixture_id) else {
        return Ok(None);
    };
    let mut state = snapshot.state;

    // ── Whole-history formation ─────────────────────────────────────────────
    let formation_tick = state.tick + 1;
    let goal_set = coherence_judge_goal_set(&state, &fixture);
    let prefix_hash = live_goal_formation_stable_prefix_hash(&goal_set);
    let judge = ModelBackedLiveGoalFormationJudge::new(client);
    let formation_started_at = time::OffsetDateTime::now_utc();
    let outcome = judge.form_and_detect(context, &goal_set, whole_history_input)?;
    let formation_completed_at = time::OffsetDateTime::now_utc();

    let proposed_candidate_json;
    let (formation_events, resolution) = match &outcome.proposed_candidate {
        Some(candidate) => {
            let candidate_tier = effective_tier_from_tension_ids(candidate.tension_ids(), &fixture);
            proposed_candidate_json = json!({
                "goal_id": candidate.id(),
                "title": candidate.title(),
                "effective_tier": candidate_tier,
                "tension_ids": candidate.tension_ids(),
            });
            let (events, resolution) = resolve_formed_candidate(
                candidate,
                &outcome.verdict,
                &state,
                &fixture,
                formation_tick,
            );
            (events, Some(resolution))
        }
        None => {
            proposed_candidate_json = serde_json::Value::Null;
            (Vec::new(), None)
        }
    };
    let hard_tier_floor_rejected = resolution
        .as_ref()
        .map(|resolution| matches!(resolution, AdmissionResolution::RejectProtectedFloor));

    let state_before_formation = state.clone();
    for event in formation_events.clone() {
        state = apply(state, event);
    }
    let admitted_goal_id = matches!(
        resolution,
        Some(AdmissionResolution::Judged { admitted: true, .. })
    )
    .then(|| {
        outcome
            .proposed_candidate
            .as_ref()
            .map(|c| c.id().to_string())
    })
    .flatten();
    let declined_candidate = outcome.proposed_candidate.as_ref().and_then(|candidate| {
        newly_declined_candidate(&state_before_formation, &state, candidate.id())
    });
    let (formation_status_before, formation_status_after) =
        goal_status_diff(&formation_events, &state_before_formation, &state);

    let formation_details = json!({
        "trigger": "sleep_formation",
        "tick": formation_tick,
        "input_ref": whole_history_input_ref(session_id),
        "cached_prefix_ref": prefix_hash,
        "prefix_cache_eligible": false,
        "judge_ref": outcome.verdict.judge_ref,
        "proposed_candidate": proposed_candidate_json,
        "goal_set_snapshot": goal_set_snapshot_json(&state_before_formation, &fixture, &goal_set),
        "contradictions": outcome.verdict.contradictions,
        "hard_tier_floor_rejected": hard_tier_floor_rejected,
        "resolution": resolution.as_ref().map(|r| json!(r)).unwrap_or(serde_json::Value::Null),
        "declined_candidate": declined_candidate.as_ref().map(|d| json!(d)),
        "response_dispatch_ref": serde_json::Value::Null,
        "response_dispatched_at": serde_json::Value::Null,
        "formation_started_at": formation_started_at.unix_timestamp_nanos(),
        "formation_completed_at": formation_completed_at.unix_timestamp_nanos(),
        "events_emitted": formation_events,
        "goal_status_before": formation_status_before,
        "goal_status_after": formation_status_after,
        "artifact_or_record_reference": format!("live-goal-formation#trigger=sleep_formation&tick={formation_tick}"),
    });
    let formation_trace_id = write_volition_trace(
        context,
        "live-goal-formation",
        "sleep_formation",
        formation_tick,
        &formation_events,
        formation_details,
    )?;
    context.record_event(
        EventType::TraceRecorded,
        json!({
            "trigger": "sleep_formation",
            "tick": formation_tick,
            "proposed_candidate": outcome.proposed_candidate.as_ref().map(|c| c.id()),
            "events_emitted": formation_events.len(),
        }),
        Some(formation_trace_id),
    )?;

    // ── Whole-set sweep ─────────────────────────────────────────────────────
    let sweep_tick = state.tick + 1;
    let sweep_goal_set = coherence_judge_goal_set(&state, &fixture);
    let sweep_judge = ModelBackedCoherenceJudge::new(client);
    let verdict = sweep_judge.judge(context, &sweep_goal_set)?;
    let (sweep_resolution, sweep_events) = resolve_sweep(&verdict, &state, &fixture, sweep_tick);

    let state_before_sweep = state.clone();
    for event in sweep_events.clone() {
        state = apply(state, event);
    }
    let swept_goal_ids: Vec<String> = sweep_resolution
        .cancellations
        .iter()
        .map(|cancellation| cancellation.cancelled_goal_id.clone())
        .collect();
    let (sweep_status_before, sweep_status_after) =
        goal_status_diff(&sweep_events, &state_before_sweep, &state);

    let sweep_details = json!({
        "trigger": "sweep",
        "tick": sweep_tick,
        "candidate": null,
        "goal_set_snapshot": goal_set_snapshot_json(&state_before_sweep, &fixture, &sweep_goal_set),
        "judge_ref": verdict.judge_ref,
        "contradictions": verdict.contradictions,
        "hard_tier_floor_rejected": null,
        "resolution": sweep_resolution,
        "events_emitted": sweep_events,
        "goal_status_before": sweep_status_before,
        "goal_status_after": sweep_status_after,
        "artifact_or_record_reference": format!("goal-coherence-check#trigger=sweep&tick={sweep_tick}"),
    });
    let sweep_trace_id =
        write_coherence_trace(context, "sweep", sweep_tick, &sweep_events, sweep_details)?;
    context.record_event(
        EventType::TraceRecorded,
        json!({
            "trigger": "sweep",
            "tick": sweep_tick,
            "cancelled_goal_ids": swept_goal_ids,
            "flagged_pairs": sweep_resolution.flagged.len(),
            "events_emitted": sweep_events.len(),
        }),
        Some(sweep_trace_id),
    )?;

    // Persist the consolidated snapshot so the next session resumes with the maintained goals.
    let inspection = build_state_inspection(&state, &fixture);
    let updated_snapshot = VolitionContinuitySnapshot::new(
        snapshot.qsf_session_id,
        snapshot.recorded_at,
        snapshot.seed_fixture_id,
        state,
        inspection,
    );
    persist_volition_continuity_snapshot(&updated_snapshot, &snapshot_path)?;

    Ok(Some(SleepVolitionMaintenanceSummary {
        admitted_goal_id,
        declined_candidate,
        swept_goal_ids,
        persisted_snapshot_path: Some(snapshot_path),
    }))
}

fn whole_history_input_ref(session_id: &str) -> String {
    format!("sleep-input-bundle#session={session_id}")
}

fn load_reviewed_seed(path: &Path) -> anyhow::Result<ReviewedVolitionSeed> {
    load_reviewed_volition_seed(path)
        .with_context(|| format!("failed to load reviewed volition seed `{}`", path.display()))
}

/// Parse `VolitionTurnOutcome` records from a diagnostics JSONL file.
///
/// Dispatches on `RecordEnvelope::kind` and deserializes only initiative records. The ledger is
/// append-only across builds, so a line of some other kind that this build cannot parse — an
/// unknown kind, or a known kind in an older shape — must not fail the read: none of it is input to
/// sleep. A malformed *initiative* record is a different matter and still fails, with its line
/// number, because that one is input.
///
/// `recorded_at` lives at the record level rather than inside the trace, and the trace names its
/// reference field `artifact_or_record_reference`, so this function maps the two shapes explicitly.
pub(crate) fn load_initiative_outcomes(path: &Path) -> anyhow::Result<Vec<VolitionTurnOutcome>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read diagnostics JSONL `{}`", path.display()))?;
    let mut outcomes = Vec::new();
    for (line_index, line) in contents.lines().enumerate() {
        let Some(envelope) = decode_envelope(line) else {
            continue;
        };
        if envelope.kind.as_deref() != Some(REALTIME_BOUNDED_INITIATIVE_KIND) {
            continue;
        }
        let record: DiagnosticRecord = serde_json::from_str(line).with_context(|| {
            format!(
                "failed to parse initiative record on line {} of `{}`",
                line_index + 1,
                path.display()
            )
        })?;
        let DiagnosticRecord::RealtimeBoundedInitiative {
            recorded_at, trace, ..
        } = record
        else {
            anyhow::bail!(
                "line {} of `{}` carries kind `{REALTIME_BOUNDED_INITIATIVE_KIND}` but did not \
                 deserialize as that variant",
                line_index + 1,
                path.display()
            );
        };
        outcomes.push(VolitionTurnOutcome {
            qsf_session_id: trace.qsf_session_id,
            exchange_index: trace.exchange_index,
            recorded_at: recorded_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            response_create_event_ref: trace.response_create_event_ref,
            winning_goal_id: trace.winning_goal_id,
            initiative_output: trace.initiative_output,
            surfaced: trace.surfaced,
            suppression_reason: trace.suppression_reason,
            rendered_line_present: trace.rendered_line_present,
            artifact_reference: trace.artifact_or_record_reference,
        });
    }
    Ok(outcomes)
}

pub(crate) fn render_markdown_report(report: &VolitionConsolidationReport) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Volition Continuity Report\n\n");
    markdown.push_str(&format!(
        "- Session: `{}`\n- Seed fixture: `{}`\n- Items: `{}`\n\n",
        report.qsf_session_id,
        report.seed_fixture_id,
        report.items.len()
    ));
    for item in &report.items {
        markdown.push_str(&format!(
            "- `{:?}` `{}` count={} ref=`{}`\n",
            item.kind, item.goal_or_candidate_id, item.count, item.artifact_reference
        ));
        if let Some(range) = item.tick_range {
            markdown.push_str(&format!(
                "  - ticks {}..{}\n",
                range.first_tick, range.last_tick
            ));
        }
        if let Some(status) = item.promotion_status {
            markdown.push_str(&format!("  - promotion_status: `{:?}`\n", status));
        }
        if let Some(decision) = item.candidate_decision {
            markdown.push_str(&format!("  - decision: `{:?}`\n", decision));
        }
        if let Some(reason) = item.suppression_reason {
            markdown.push_str(&format!("  - suppression_reason: `{reason:?}`\n"));
        }
    }
    markdown
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_diagnostics::{
        DiagnosticRecord, RealtimeBoundedInitiativeTrace, RealtimeBoundedOrExternalOutput,
    };
    use qsf_models::MockModelClient;
    use qsf_volition::{
        AllowedEffect, GoalScope, InitiativeOutput, InitiativeProposal, VolitionState,
        build_state_inspection, realtime_seed_fixture,
    };
    use tempfile::TempDir;
    use time::OffsetDateTime;

    fn write_snapshot_fixture(state_root: &Path, session_id: &str) -> anyhow::Result<()> {
        let continuity_dir = state_root.join("continuity").join(session_id);
        std::fs::create_dir_all(&continuity_dir)?;
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        let snapshot = VolitionContinuitySnapshot::new(
            session_id,
            "2026-07-02T00:00:00Z",
            REALTIME_SEED_FIXTURE_ID,
            state,
            inspection,
        );
        persist_volition_continuity_snapshot(
            &snapshot,
            continuity_dir.join("volition-state.json"),
        )?;
        let manifest = ContinuityManifest {
            current_volition_snapshot_path: Some(PathBuf::from("volition-state.json")),
            ..ContinuityManifest::default()
        };
        manifest.persist(continuity_dir.join("continuity-manifest.json"))?;
        Ok(())
    }

    #[test]
    fn sleep_goal_maintenance_runs_end_to_end_with_the_default_mock_client() {
        let state_root = TempDir::new().unwrap();
        write_snapshot_fixture(state_root.path(), "session-1").unwrap();

        let mut context =
            RunContext::create_in(state_root.path().join("runs"), "sleep-maintenance-test")
                .unwrap();
        let client = MockModelClient::default();

        let summary = run_sleep_volition_goal_maintenance(
            &mut context,
            &client,
            state_root.path(),
            "session-1",
            "The whole-session summary.",
        )
        .unwrap()
        .expect("maintenance runs when a snapshot exists");

        // The default mock forms nothing and detects no contradictions.
        assert!(summary.admitted_goal_id.is_none());
        assert!(summary.declined_candidate.is_none());
        assert!(summary.swept_goal_ids.is_empty());

        // Both trace records were written, so the sleep pass is reconstructable from traces.
        let traces = std::fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let operations: Vec<String> = traces
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["operation"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert!(operations.iter().any(|op| op == "live-goal-formation"));
        assert!(operations.iter().any(|op| op == "goal-coherence-check"));

        // The snapshot was re-persisted and still parses.
        let snapshot_path = state_root
            .path()
            .join("continuity")
            .join("session-1")
            .join("volition-state.json");
        assert!(VolitionContinuitySnapshot::load_or_upgrade(&snapshot_path).is_ok());
    }

    /// A judge client whose formation response depends on whether the request actually contains
    /// `marker` - used to prove `run_sleep_volition_goal_maintenance` forms a candidate from
    /// whatever text it is given, so the caller passing a summary (which may omit a durable goal
    /// signal) instead of the raw session transcript is a caller-side bug, not a judge limitation.
    struct MarkerConditionedFormationClient {
        marker: &'static str,
    }

    impl qsf_models::ModelClient for MarkerConditionedFormationClient {
        fn client_name(&self) -> &str {
            "marker-conditioned-formation-client"
        }

        fn complete(
            &self,
            request: &qsf_models::ModelRequest,
        ) -> anyhow::Result<qsf_models::ModelResponse> {
            let saw_marker = request
                .messages
                .iter()
                .any(|message| message.content.contains(self.marker));
            let body = if saw_marker {
                json!({
                    "proposed_candidate": {
                        "id": "durable-goal-from-history",
                        "title": "Durable goal from history",
                        "summary": "Formed from the whole-history transcript.",
                        "tension_ids": [],
                        "scope": "session",
                        "base_priority": 5,
                        "allowed_effects": ["reflect"],
                        "satisfaction_condition_summary": "satisfied when honored",
                        "proposal_evidence": ["whole-history transcript"],
                        "source_description": "formed from sleep whole-history formation",
                        "activation_keywords": []
                    },
                    "contradictions": []
                })
            } else {
                json!({ "proposed_candidate": null, "contradictions": [] })
            };
            Ok(qsf_models::ModelResponse::from_text(
                request,
                self.client_name(),
                request.model_name.clone(),
                body.to_string(),
            ))
        }
    }

    #[test]
    fn sleep_whole_history_formation_proposes_a_candidate_omitted_from_a_summary() {
        let state_root = TempDir::new().unwrap();
        write_snapshot_fixture(state_root.path(), "session-1").unwrap();
        let marker = "always double-check the backups before deploying";

        let mut context =
            RunContext::create_in(state_root.path().join("runs"), "sleep-whole-history-test")
                .unwrap();
        let client = MarkerConditionedFormationClient { marker };

        // A generic summary (as a lossy summarizer might produce) does not contain the marker,
        // so formation over the summary alone would see nothing durable to propose.
        let summary_only = run_sleep_volition_goal_maintenance(
            &mut context,
            &client,
            state_root.path(),
            "session-1",
            "Nothing of particular note happened this session.",
        )
        .unwrap()
        .expect("maintenance runs when a snapshot exists");
        assert!(summary_only.admitted_goal_id.is_none());

        // Reset the snapshot so the second call starts from the same state as the first.
        write_snapshot_fixture(state_root.path(), "session-1").unwrap();

        // The raw session transcript carries the durable goal signal the summary dropped.
        let whole_history = format!(
            "Turn 0:\nUser: Please {marker}.\nAssistant: Understood, I will keep that in mind."
        );
        let from_whole_history = run_sleep_volition_goal_maintenance(
            &mut context,
            &client,
            state_root.path(),
            "session-1",
            &whole_history,
        )
        .unwrap()
        .expect("maintenance runs when a snapshot exists");

        assert_eq!(
            from_whole_history.admitted_goal_id.as_deref(),
            Some("durable-goal-from-history")
        );
    }

    #[test]
    fn sleep_goal_maintenance_returns_none_without_a_snapshot() {
        let state_root = TempDir::new().unwrap();
        let mut context =
            RunContext::create_in(state_root.path().join("runs"), "sleep-maintenance-none")
                .unwrap();
        let client = MockModelClient::default();

        let result = run_sleep_volition_goal_maintenance(
            &mut context,
            &client,
            state_root.path(),
            "absent-session",
            "summary",
        )
        .unwrap();

        assert!(result.is_none());
    }

    /// Serializes a real `DiagnosticRecord::RealtimeBoundedInitiative` so the test exercises the
    /// same wire format the realtime server writes.
    fn minimal_initiative_jsonl_line(
        suppression_reason: Option<qsf_volition::VolitionSuppressionReason>,
        surfaced: bool,
        artifact_or_record_reference: &str,
        recorded_at: time::OffsetDateTime,
    ) -> String {
        let output = InitiativeOutput::ReflectionRequested {
            proposed_question: "What next?".to_string(),
        };
        let inspection = build_state_inspection(
            &VolitionState::from_fixture(&realtime_seed_fixture()),
            &realtime_seed_fixture(),
        );
        let record = DiagnosticRecord::RealtimeBoundedInitiative {
            qsf_session_id: "session-1".to_string(),
            exchange_index: 3,
            recorded_at,
            trace: RealtimeBoundedInitiativeTrace {
                qsf_session_id: "session-1".to_string(),
                exchange_index: 3,
                winning_goal_id: "serve-the-present-person".to_string(),
                initiative_proposal: InitiativeProposal {
                    goal_id: "serve-the-present-person".to_string(),
                    goal_title: "Serve".to_string(),
                    effect: AllowedEffect::Reflect,
                    rationale: "test".to_string(),
                    matched_terms: vec![],
                    scope: GoalScope::Input,
                },
                allowed_effect: AllowedEffect::Reflect,
                initiative_output: output.clone(),
                bounded_or_external_output: RealtimeBoundedOrExternalOutput {
                    initiative_output: output,
                    external_effect_executed: false,
                },
                surfaced,
                suppression_reason,
                rendered_line_present: surfaced,
                context_retrieval_hint_terms: None,
                hint_consumed_by_next_memory_injection: false,
                rationale: "test".to_string(),
                state_snapshot_before: inspection.clone(),
                state_snapshot_after: inspection,
                response_create_event_ref: "ref-1".to_string(),
                artifact_or_record_reference: artifact_or_record_reference.to_string(),
            },
        };
        serde_json::to_string(&record).expect("serialize initiative record")
    }

    #[test]
    fn load_initiative_outcomes_parses_record_level_recorded_at_and_maps_artifact_reference() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let line = minimal_initiative_jsonl_line(
            None,
            true,
            "exchange:3/diagnostic:realtime_bounded_initiative",
            time::OffsetDateTime::parse(
                "2026-06-30T12:00:00Z",
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap(),
        );
        fs::write(&path, &line).unwrap();

        let outcomes = load_initiative_outcomes(&path).unwrap();

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].recorded_at, "2026-06-30T12:00:00Z");
        assert_eq!(
            outcomes[0].artifact_reference,
            "exchange:3/diagnostic:realtime_bounded_initiative"
        );
        assert_eq!(outcomes[0].exchange_index, 3);
        assert!(outcomes[0].surfaced);
        assert!(outcomes[0].suppression_reason.is_none());
    }

    #[test]
    fn load_initiative_outcomes_round_trips_all_suppression_reasons() {
        use qsf_volition::VolitionSuppressionReason;

        let cases: &[(&str, VolitionSuppressionReason)] = &[
            ("intensity", VolitionSuppressionReason::Intensity),
            (
                "protected_no_opportunity",
                VolitionSuppressionReason::ProtectedNoOpportunity,
            ),
            ("anti_nag_repeat", VolitionSuppressionReason::AntiNagRepeat),
            (
                "non_renderable_output",
                VolitionSuppressionReason::NonRenderableOutput,
            ),
        ];

        let dir = TempDir::new().unwrap();
        for (reason_str, expected) in cases {
            let path = dir.path().join(format!("{reason_str}.jsonl"));
            let line = minimal_initiative_jsonl_line(
                Some(*expected),
                false,
                "exchange:3/diagnostic:realtime_bounded_initiative",
                time::OffsetDateTime::parse(
                    "2026-06-30T12:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .unwrap(),
            );
            fs::write(&path, &line).unwrap();

            let outcomes = load_initiative_outcomes(&path).unwrap();

            assert_eq!(outcomes.len(), 1, "missing outcome for reason {reason_str}");
            assert_eq!(
                outcomes[0].suppression_reason,
                Some(*expected),
                "wrong reason for {reason_str}"
            );
            assert!(!outcomes[0].surfaced);
        }
    }

    #[test]
    fn load_initiative_outcomes_skips_non_initiative_records() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mixed.jsonl");
        let other =
            r#"{"kind":"session_allocated","qsf_session_id":"s","at":"2026-06-30T00:00:00Z"}"#;
        let initiative = minimal_initiative_jsonl_line(
            None,
            true,
            "exchange:0/diagnostic:realtime_bounded_initiative",
            time::OffsetDateTime::parse(
                "2026-06-30T12:00:00Z",
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap(),
        );
        fs::write(&path, format!("{other}\n{initiative}\n")).unwrap();

        let outcomes = load_initiative_outcomes(&path).unwrap();

        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn unrelated_and_unknown_record_kinds_do_not_prevent_reading_initiative_outcomes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let initiative =
            minimal_initiative_jsonl_line(None, true, "artifact-1", OffsetDateTime::UNIX_EPOCH);
        std::fs::write(
            &path,
            format!(
                // A kind from a future build, a known kind in an older shape (missing fields this
                // build requires), and then a valid initiative record.
                "{{\"kind\":\"from_a_future_build\",\"anything\":true}}\n\
                 {{\"kind\":\"live_goal_formation_skipped\",\"qsf_session_id\":\"s\"}}\n\
                 {initiative}\n"
            ),
        )
        .unwrap();

        let outcomes = load_initiative_outcomes(&path).expect("unrelated kinds must not abort");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].artifact_reference, "artifact-1");
    }

    #[test]
    fn a_malformed_initiative_record_still_fails_with_its_line_number() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        std::fs::write(
            &path,
            "{\"kind\":\"from_a_future_build\"}\n\
             {\"kind\":\"realtime_bounded_initiative\",\"trace\":{\"nope\":true}}\n",
        )
        .unwrap();

        let error = load_initiative_outcomes(&path).expect_err("a bad initiative record must fail");

        assert!(
            error.to_string().contains("line 2"),
            "error must name the failing line: {error}"
        );
    }
}
