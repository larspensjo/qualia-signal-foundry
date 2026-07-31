use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    ProbeRunState, ProbeVerdict, RuntimeCounters, SecretScanReport, StructuralComparison,
    TraceContractReport,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeManifestMetadata {
    pub attachment_shape: String,
    pub reconnect_policy: String,
    pub model_ids: ModelIds,
    pub world_corpus: WorldCorpusManifest,
    pub seed_mode: SeedMode,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelIds {
    pub realtime_voice: String,
    pub input_transcription: Option<String>,
    pub live_goal_formation: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldCorpusManifest {
    pub state: String,
    pub marker: Option<serde_json::Value>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SeedMode {
    ColdStart,
    NoSeedBundleConfigured,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnExpectationDiff {
    pub exchange_index: usize,
    pub differences: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunManifest {
    pub status: super::ProbeStatus,
    pub run_id: String,
    pub phrase_set_id: String,
    pub phrase_set_hash: String,
    pub phrase_count: usize,
    pub state_dir: String,
    pub git_commit: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub attachment_shape: String,
    pub reconnect_policy: String,
    pub model_ids: ModelIds,
    pub world_corpus: WorldCorpusManifest,
    pub seed_mode: SeedMode,
    pub run_state: ProbeRunState,
    pub runtime_counters: RuntimeCounters,
    pub trace_contract: TraceContractReport,
    pub structural_comparison: StructuralComparison,
    pub secret_scan: SecretScanReport,
    pub failing_clauses: Vec<String>,
    pub structured_clauses: Vec<String>,
    pub expectation_diff: Vec<TurnExpectationDiff>,
    pub original_failure: Option<String>,
    pub finalization_errors: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_run_manifest(
    run_id: String,
    phrase_set_id: String,
    phrase_set_hash: String,
    state_dir: String,
    git_commit: Option<String>,
    started_at: String,
    finished_at: String,
    metadata: ProbeManifestMetadata,
    run_state: ProbeRunState,
    runtime_counters: RuntimeCounters,
    trace_contract: TraceContractReport,
    structural_comparison: StructuralComparison,
    secret_scan: SecretScanReport,
    verdict: ProbeVerdict,
    original_failure: Option<String>,
    finalization_errors: Vec<String>,
) -> RunManifest {
    let expectation_diff = run_state
        .turns
        .iter()
        .filter(|turn| !turn.expectation_differences.is_empty())
        .map(|turn| TurnExpectationDiff {
            exchange_index: turn.index,
            differences: turn.expectation_differences.clone(),
        })
        .collect();
    RunManifest {
        status: verdict.status,
        run_id,
        phrase_set_id,
        phrase_set_hash,
        phrase_count: runtime_counters.phrase_count,
        state_dir,
        git_commit,
        started_at,
        finished_at,
        attachment_shape: metadata.attachment_shape,
        reconnect_policy: metadata.reconnect_policy,
        model_ids: metadata.model_ids,
        world_corpus: metadata.world_corpus,
        seed_mode: metadata.seed_mode,
        run_state,
        runtime_counters,
        trace_contract,
        structural_comparison,
        secret_scan,
        failing_clauses: verdict.failing_clauses,
        structured_clauses: verdict.structured_clauses,
        expectation_diff,
        original_failure,
        finalization_errors,
    }
}

pub fn serialize_manifest(manifest: &RunManifest) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(manifest)?)
}

pub fn write_manifest_atomic(path: &Path, manifest: &RunManifest) -> anyhow::Result<()> {
    let bytes = serialize_manifest(manifest)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("manifest path has no parent"))?;
    let temporary = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripted_conversation::{
        ProbeRunState, ProbeStatus, ProbeVerdict, RuntimeCounters, SecretScanReport,
        StructuralComparison, TraceContractReport,
    };
    #[test]
    fn atomic_write_replaces_terminal_document() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("run-manifest.json");
        let verdict = ProbeVerdict {
            status: ProbeStatus::Passed,
            failing_clauses: vec![],
            structured_clauses: vec![],
        };
        let metadata = ProbeManifestMetadata {
            attachment_shape: "server_model_session".into(),
            reconnect_policy: "fail_closed_after_first_attach".into(),
            model_ids: ModelIds {
                realtime_voice: "realtime".into(),
                input_transcription: Some("transcription".into()),
                live_goal_formation: "formation".into(),
            },
            world_corpus: WorldCorpusManifest {
                state: "ready".into(),
                marker: Some(serde_json::json!({"schema_version": 1})),
                detail: None,
            },
            seed_mode: SeedMode::ColdStart,
        };
        let first = build_run_manifest(
            "first".into(),
            "p".into(),
            "h".into(),
            "d".into(),
            None,
            "s".into(),
            "f".into(),
            metadata.clone(),
            ProbeRunState::default(),
            RuntimeCounters::default(),
            TraceContractReport::default(),
            StructuralComparison::default(),
            SecretScanReport::default(),
            verdict.clone(),
            Some("first failure".into()),
            vec![],
        );
        write_manifest_atomic(&path, &first).expect("write first");
        let second = build_run_manifest(
            "second".into(),
            "p".into(),
            "h".into(),
            "d".into(),
            None,
            "s".into(),
            "f".into(),
            metadata,
            ProbeRunState::default(),
            RuntimeCounters::default(),
            TraceContractReport::default(),
            StructuralComparison::default(),
            SecretScanReport::default(),
            verdict,
            None,
            vec![],
        );
        write_manifest_atomic(&path, &second).expect("replace");
        let parsed: RunManifest =
            serde_json::from_slice(&fs::read(&path).expect("read")).expect("parse");
        assert_eq!(parsed.run_id, "second");
        assert_eq!(parsed.git_commit, None);
        assert!(parsed.original_failure.is_none());
        assert_eq!(parsed.attachment_shape, "server_model_session");
        assert!(parsed.runtime_counters.token_ledger.models.is_empty());
        assert!(!tempdir.path().join(".run-manifest.json.tmp").exists());
        let serialized = serialize_manifest(&second).expect("serialize");
        assert_eq!(serialized, fs::read(path).expect("stable bytes"));
    }
}
