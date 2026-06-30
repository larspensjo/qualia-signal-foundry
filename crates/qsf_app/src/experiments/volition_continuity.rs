use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use serde_json::Value;

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::session::{
    ContinuityManifest, StateDirectoryResolution, resolve_shared_state_directory_from_env,
};
use qsf_volition::{
    REALTIME_SEED_FIXTURE_ID, ReviewedVolitionSeed, VolitionConsolidationReport,
    VolitionConsolidationSnapshotRecord, VolitionContinuitySnapshot, VolitionTurnOutcome,
    build_volition_consolidation_report, load_reviewed_volition_seed,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

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
fn snapshot_path_from_manifest(
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

/// Read the realtime continuity artifacts for `session_id` from `state_root` and run pure
/// consolidation. Returns `None` when no continuity snapshot exists for that session (i.e. no
/// realtime volition continuity has been persisted yet).
///
/// Called both from `VolitionContinuityExperiment` and from the sleep-pass
/// `commit_cross_session_sleep` flow so consolidation runs through the standard sleep path.
pub(crate) fn consolidate_session_volition(
    state_root: &Path,
    session_id: &str,
) -> anyhow::Result<Option<VolitionConsolidationReport>> {
    let continuity_dir = state_root.join("continuity").join(session_id);
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

    let diagnostics_path = state_root
        .join("diagnostics")
        .join(format!("{session_id}.jsonl"));
    let initiative_outcomes = load_initiative_outcomes(&diagnostics_path)?;

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

fn load_reviewed_seed(path: &Path) -> anyhow::Result<ReviewedVolitionSeed> {
    load_reviewed_volition_seed(path)
        .with_context(|| format!("failed to load reviewed volition seed `{}`", path.display()))
}

/// Parse `VolitionTurnOutcome` records from a diagnostics JSONL file, skipping non-initiative
/// records. Merges record-level `recorded_at` and renames `artifact_or_record_reference` to
/// `artifact_reference` to match `VolitionTurnOutcome`'s field layout.
pub(crate) fn load_initiative_outcomes(path: &Path) -> anyhow::Result<Vec<VolitionTurnOutcome>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read diagnostics JSONL `{}`", path.display()))?;
    let mut outcomes = Vec::new();
    for (line_index, line) in contents.lines().enumerate() {
        let record: Value = serde_json::from_str(line).with_context(|| {
            format!(
                "failed to parse diagnostics line {} from `{}`",
                line_index + 1,
                path.display()
            )
        })?;
        if record.get("kind").and_then(Value::as_str) != Some("realtime_bounded_initiative") {
            continue;
        }
        let Some(trace) = record.get("trace") else {
            continue;
        };

        // `recorded_at` lives at the record level (not inside the trace), and the trace
        // names its reference field `artifact_or_record_reference` while `VolitionTurnOutcome`
        // expects `artifact_reference`. Merge both fixes into a single JSON object before
        // deserializing.
        let recorded_at = record
            .get("recorded_at")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let mut merged = trace.clone();
        if let Value::Object(ref mut map) = merged {
            map.entry("recorded_at".to_string()).or_insert(recorded_at);
            if !map.contains_key("artifact_reference") {
                if let Some(v) = map.remove("artifact_or_record_reference") {
                    map.insert("artifact_reference".to_string(), v);
                }
            }
        }

        let outcome: VolitionTurnOutcome = serde_json::from_value(merged).with_context(|| {
            format!(
                "failed to parse volition initiative trace from `{}`",
                path.display()
            )
        })?;
        outcomes.push(outcome);
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
    use tempfile::TempDir;

    /// Constructs a minimal `realtime_bounded_initiative` JSONL line matching the wire format
    /// emitted by `DiagnosticRecord::RealtimeBoundedInitiative` (which cannot be imported here
    /// because `qsf_app` must not depend on `qsf_realtime_server`).
    fn minimal_initiative_jsonl_line(
        suppression_reason: Option<&str>,
        surfaced: bool,
        artifact_or_record_reference: &str,
        recorded_at: &str,
    ) -> String {
        let suppression = match suppression_reason {
            None => "null".to_string(),
            Some(r) => format!(r#""{r}""#),
        };
        format!(
            r#"{{"kind":"realtime_bounded_initiative","qsf_session_id":"session-1","exchange_index":3,"recorded_at":"{recorded_at}","trace":{{"qsf_session_id":"session-1","exchange_index":3,"winning_goal_id":"honor-explicit-user-request","initiative_proposal":{{"goal_id":"honor-explicit-user-request","goal_title":"Honor","effect":"reflect","rationale":"test","matched_terms":[],"scope":"input"}},"allowed_effect":"reflect","initiative_output":{{"kind":"reflection_requested","proposed_question":"What next?"}},"bounded_or_external_output":{{"initiative_output":{{"kind":"reflection_requested","proposed_question":"What next?"}},"external_effect_executed":false}},"surfaced":{surfaced},"suppression_reason":{suppression},"rendered_line_present":{surfaced},"context_retrieval_hint_terms":null,"hint_consumed_by_next_memory_injection":false,"rationale":"test","state_snapshot_before":{{"tick":0,"mode":"neutral","accepted_goal_count":0,"active_goal_count":0,"pending_candidate_count":0,"blocked_goals":[],"last_initiative_summaries":[]}},"state_snapshot_after":{{"tick":0,"mode":"neutral","accepted_goal_count":0,"active_goal_count":0,"pending_candidate_count":0,"blocked_goals":[],"last_initiative_summaries":[]}},"response_create_event_ref":"ref-1","artifact_or_record_reference":"{artifact_or_record_reference}"}}}}"#
        )
    }

    #[test]
    fn load_initiative_outcomes_parses_record_level_recorded_at_and_maps_artifact_reference() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let line = minimal_initiative_jsonl_line(
            None,
            true,
            "exchange:3/diagnostic:realtime_bounded_initiative",
            "2026-06-30T12:00:00Z",
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
                Some(reason_str),
                false,
                "exchange:3/diagnostic:realtime_bounded_initiative",
                "2026-06-30T12:00:00Z",
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
            "2026-06-30T12:00:00Z",
        );
        fs::write(&path, format!("{other}\n{initiative}\n")).unwrap();

        let outcomes = load_initiative_outcomes(&path).unwrap();

        assert_eq!(outcomes.len(), 1);
    }
}
