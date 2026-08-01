use serde::{Deserialize, Serialize};

use crate::realtime::token_usage::TokenUsageSnapshot;

use super::{ProbeRunState, SecretScanReport, TraceContractReport};

/// Placeholder for the later artifact-structure builder. It deliberately says why no comparison
/// was made, so adding a real reference does not change the verdict API.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum StructuralComparison {
    #[default]
    NoStructuralReferenceConfigured,
    Matches,
    Divergence {
        details: Vec<String>,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCounters {
    pub phrase_count: usize,
    pub session_created: bool,
    pub promoted_turn_count: usize,
    pub promoted_exchange_indices: Vec<usize>,
    pub non_promotable_exchange_indices: Vec<usize>,
    pub degradation_epoch: u32,
    pub degradation_reasons: Vec<String>,
    pub terminated_reason: Option<String>,
    pub output_audio_delta_count: u64,
    pub output_audio_delta_byte_count: u64,
    pub token_ledger: TokenUsageSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    Passed,
    Failed,
    InfrastructureError,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeVerdict {
    pub status: ProbeStatus,
    pub failing_clauses: Vec<String>,
    pub structured_clauses: Vec<String>,
}

pub fn probe_verdict(
    state: &ProbeRunState,
    counters: &RuntimeCounters,
    traces: &TraceContractReport,
    structure: &StructuralComparison,
    secrets: &SecretScanReport,
    finalization_errors: &[String],
    original_failure: Option<&str>,
) -> ProbeVerdict {
    let mut failing = Vec::new();
    if state.attach_timed_out {
        failing.push("attach_timeout".to_string());
    }
    for turn in &state.turns {
        if turn.timed_out {
            failing.push(format!("turn_timeout:{}", turn.index));
        }
    }
    if let Some(reason) = state
        .terminated_reason
        .as_ref()
        .or(counters.terminated_reason.as_ref())
    {
        failing.push(format!("sideband_terminated:{reason}"));
    }
    if counters.session_created && counters.promoted_turn_count != counters.phrase_count {
        failing.push(format!(
            "promoted_turn_count:{}!=phrase_count:{}",
            counters.promoted_turn_count, counters.phrase_count
        ));
    }
    if !counters.non_promotable_exchange_indices.is_empty() {
        failing.push("non_promotable_exchange".to_string());
    }
    if counters.degradation_epoch > 0 {
        failing.push("degradation_epoch".to_string());
    }
    if !traces.complete {
        failing.push("trace_contract".to_string());
    }
    if matches!(structure, StructuralComparison::Divergence { .. }) {
        failing.push("structural_divergence".to_string());
    }
    if secrets.found {
        failing.push("secret_detected".to_string());
    }
    let mut structured = Vec::new();
    if let Some(formation) = &state.formation {
        if formation.timed_out {
            structured.push("formation_timed_out".to_string());
        }
        if formation.failed > 0 {
            structured.push(format!("formation_failed:{}", formation.failed));
        }
    }
    for turn in &state.turns {
        if !turn.expectation_differences.is_empty() {
            structured.push(format!(
                "expectation diff at turn {}: {}",
                turn.index + 1,
                turn.expectation_differences.join("; ")
            ));
        }
    }
    let status = if !failing.is_empty() {
        ProbeStatus::Failed
    } else if !finalization_errors.is_empty() || original_failure.is_some() {
        ProbeStatus::InfrastructureError
    } else {
        ProbeStatus::Passed
    };
    ProbeVerdict {
        status,
        failing_clauses: failing,
        structured_clauses: structured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripted_conversation::{ProbeEvent, reduce};

    fn clean() -> (
        ProbeRunState,
        RuntimeCounters,
        TraceContractReport,
        SecretScanReport,
    ) {
        let mut state = ProbeRunState::default();
        reduce(&mut state, ProbeEvent::SidebandAttached);
        (
            state,
            RuntimeCounters {
                phrase_count: 0,
                ..RuntimeCounters::default()
            },
            crate::scripted_conversation::parse_trace_contract(b"", &[]),
            SecretScanReport::default(),
        )
    }

    fn verdict(
        state: &ProbeRunState,
        counters: &RuntimeCounters,
        traces: &TraceContractReport,
        secrets: &SecretScanReport,
    ) -> ProbeVerdict {
        probe_verdict(
            state,
            counters,
            traces,
            &StructuralComparison::default(),
            secrets,
            &[],
            None,
        )
    }

    #[test]
    fn clean_run_passes() {
        let (state, counters, traces, secrets) = clean();
        assert_eq!(
            verdict(&state, &counters, &traces, &secrets).status,
            ProbeStatus::Passed
        );
    }

    #[test]
    fn deterministic_failures_have_specific_clauses() {
        let cases = [
            ("attach_timeout", ProbeEvent::AttachTimedOut),
            ("turn_timeout:1", ProbeEvent::TurnTimedOut { index: 1 }),
            (
                "sideband_terminated:closed",
                ProbeEvent::SidebandTerminated {
                    reason: "closed".to_string(),
                },
            ),
        ];
        for (clause, event) in cases {
            let (mut state, counters, traces, secrets) = clean();
            reduce(&mut state, event);
            assert!(
                verdict(&state, &counters, &traces, &secrets)
                    .failing_clauses
                    .contains(&clause.to_string()),
                "missing {clause}"
            );
        }
    }

    #[test]
    fn runtime_counter_failures_have_specific_clauses() {
        let (state, mut counters, traces, secrets) = clean();
        counters.session_created = true;
        counters.phrase_count = 2;
        counters.promoted_turn_count = 1;
        counters.non_promotable_exchange_indices = vec![0];
        counters.degradation_epoch = 1;

        let verdict = verdict(&state, &counters, &traces, &secrets);

        assert!(
            verdict
                .failing_clauses
                .iter()
                .any(|clause| clause.starts_with("promoted_turn_count:"))
        );
        assert!(
            verdict
                .failing_clauses
                .contains(&"non_promotable_exchange".to_string())
        );
        assert!(
            verdict
                .failing_clauses
                .contains(&"degradation_epoch".to_string())
        );
    }

    #[test]
    fn formation_is_a_non_failing_structured_partial() {
        let (mut state, counters, traces, secrets) = clean();
        reduce(
            &mut state,
            ProbeEvent::FormationBarrierTimedOut {
                expected: 1,
                settled: 0,
                failed: 1,
                timeout_ms: 10,
            },
        );
        let verdict = verdict(&state, &counters, &traces, &secrets);
        assert_eq!(verdict.status, ProbeStatus::Passed);
        assert_eq!(
            verdict.structured_clauses,
            ["formation_timed_out", "formation_failed:1"]
        );
    }

    #[test]
    fn expectation_diff_identifies_a_one_based_turn_and_its_text() {
        let (mut state, counters, traces, secrets) = clean();
        reduce(
            &mut state,
            ProbeEvent::TurnCompleted {
                index: 0,
                promoted: true,
                elapsed_ms: 1,
                inspection_captured: true,
                arbitration_winner: None,
                expectation_differences: vec!["winner expected x, observed y".to_string()],
            },
        );
        let verdict = verdict(&state, &counters, &traces, &secrets);
        assert_eq!(
            verdict.structured_clauses,
            ["expectation diff at turn 1: winner expected x, observed y"]
        );
    }

    #[test]
    fn finalization_error_promotes_only_an_otherwise_passing_verdict() {
        let (state, counters, traces, secrets) = clean();
        let infrastructure = probe_verdict(
            &state,
            &counters,
            &traces,
            &StructuralComparison::default(),
            &secrets,
            &["manifest precursor failed".to_string()],
            None,
        );
        assert_eq!(infrastructure.status, ProbeStatus::InfrastructureError);

        let mut failed_state = state;
        reduce(&mut failed_state, ProbeEvent::AttachTimedOut);
        let failed = probe_verdict(
            &failed_state,
            &counters,
            &traces,
            &StructuralComparison::default(),
            &secrets,
            &["also failed".to_string()],
            None,
        );
        assert_eq!(failed.status, ProbeStatus::Failed);
    }

    #[test]
    fn original_infrastructure_failure_is_not_mislabeled_as_finalization() {
        let (state, counters, traces, secrets) = clean();

        let verdict = probe_verdict(
            &state,
            &counters,
            &traces,
            &StructuralComparison::default(),
            &secrets,
            &[],
            Some("OPENAI_API_KEY is required"),
        );

        assert_eq!(verdict.status, ProbeStatus::InfrastructureError);
        assert!(verdict.failing_clauses.is_empty());
    }
}
