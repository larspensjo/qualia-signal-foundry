use serde::Serialize;

use qsf_diagnostics::{
    LiveGoalFormationTrace, RealtimeBoundedInitiativeTrace, VolitionContextInjectionTrace,
    WorldConsultationTrace,
};
use qsf_session::Exchange;
use qsf_volition::{
    ActivationKeyword, AdmissionResolution, AllowedEffect, Contradiction, DeclinedCandidate,
    GoalVisibility, InitiativeOutput, KeywordWeightClass, Mode, VolitionSuppressionReason,
    WorldQueryTerm,
};

/// One emitted JSONL line. The `kind` tag makes the stream self-describing, so a reader can tell a
/// run header from a turn without positional assumptions.
#[derive(Debug, Serialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptLine {
    Session(SessionLine),
    Turn(TurnLine),
}

#[derive(Debug, Serialize)]
pub struct SessionLine {
    pub session_id: String,
    pub ledger: String,
    /// 1-based position of this run within the append-only ledger.
    pub run_index: usize,
    pub run_started_at: Option<String>,
    pub turn_count: usize,
    /// Whether this run was read completely. Part of the serialized contract, not just a console
    /// warning: a saved artifact must carry its own provenance, because whoever reads the file later
    /// did not see the invocation's stderr.
    pub source: SourceIntegrity,
}

#[derive(Debug, Default, Serialize)]
pub struct SourceIntegrity {
    /// `true` when no line of this run was skipped and no trace was orphaned.
    pub complete: bool,
    pub skipped_line_count: usize,
    pub skipped_lines: Vec<SkippedLineView>,
    pub orphans: OrphanCounts,
}

/// A ledger line this build could not decode, located well enough to go back to the source.
#[derive(Debug, Serialize)]
pub struct SkippedLineView {
    pub line_number: usize,
    /// The record's `kind`, when the envelope decoded. `null` when the line was not a JSON object.
    pub kind: Option<String>,
    /// The exchange index the line belonged to, when the envelope decoded. This is what lets a
    /// specific turn be marked incomplete rather than silently losing a section.
    pub exchange_index: Option<usize>,
    pub error: String,
}

/// Traces whose `exchange_index` matched no trusted exchange in the run.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct OrphanCounts {
    pub injection: usize,
    pub formation: usize,
    pub initiative: usize,
    pub world: usize,
    pub turn_context: usize,
}

impl OrphanCounts {
    pub fn total(&self) -> usize {
        self.injection + self.formation + self.initiative + self.world + self.turn_context
    }
}

/// Optional sections are always present as `null` when the ledger has no such record for the turn,
/// so keys are stable for downstream tooling and an absent trace is visible rather than implied.
#[derive(Debug, Serialize)]
pub struct TurnLine {
    pub turn: usize,
    pub at: Option<String>,
    pub user: String,
    pub assistant: Option<String>,
    pub status: String,
    pub volition: Option<VolitionView>,
    pub initiative: Option<InitiativeView>,
    pub formation: Option<FormationView>,
    pub world: Option<WorldView>,
    /// Record kinds that the ledger holds for this turn but this build could not decode. This is
    /// what separates "the ledger never recorded a volition trace for this turn" (`volition: null`,
    /// `undecodable: []`) from "it did, and we could not read it" (`volition: null`,
    /// `undecodable: ["volition_context_injected"]`). Without it a skipped line and a genuinely
    /// quiet turn are indistinguishable in the artifact.
    pub undecodable: Vec<String>,
    /// Present only under `--full`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traces: Option<TraceBundle>,
}

#[derive(Debug, Serialize)]
pub struct VolitionView {
    pub threshold: u32,
    pub mode: Option<Mode>,
    pub winner: Option<WinnerView>,
    /// Selected goals whose match strength reached `threshold`.
    pub fired: Vec<MatchView>,
    /// Selected goals that matched but stayed under `threshold`.
    pub below_threshold: Vec<MatchView>,
    pub omitted_count: usize,
    pub suppressed_cooldown_count: usize,
    pub blocked_count: usize,
    pub subconscious_selected_count: usize,
}

#[derive(Debug, Serialize)]
pub struct WinnerView {
    pub goal: String,
    pub title: String,
    pub effective_tier: u8,
    pub biased_tier: u8,
    pub losers: usize,
}

#[derive(Debug, Serialize)]
pub struct MatchView {
    pub goal: String,
    pub strength: u32,
    /// Rendered as `term:weight_class`, the one place this view compresses rather than nests,
    /// because this list is what a reader actually reads.
    pub keywords: Vec<String>,
    pub visibility: GoalVisibility,
    /// Populated from the trace's below-threshold candidate summary when present.
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InitiativeView {
    pub goal: String,
    pub effect: AllowedEffect,
    pub surfaced: bool,
    pub suppression: Option<VolitionSuppressionReason>,
    pub rendered_line_present: bool,
    pub output: InitiativeOutput,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationStatus {
    Performed,
    Failed,
    Skipped,
}

#[derive(Debug, Serialize)]
pub struct FormationView {
    pub status: FormationStatus,
    pub candidate_id: Option<String>,
    pub candidate_title: Option<String>,
    pub contradictions: Vec<Contradiction>,
    pub resolution: Option<AdmissionResolution>,
    pub declined: Option<DeclinedCandidate>,
    /// The error text for `Failed`, the guard reason for `Skipped`, `None` for `Performed`.
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorldView {
    pub serving_goal: String,
    pub serving_goal_title: String,
    /// Carried whole rather than flattened to strings: `WorldQueryTerm` is two fields and its
    /// `source` says whether the term came from goal activation or an explicit current topic.
    pub query_terms: Vec<WorldQueryTerm>,
    pub surfaced_facts: Vec<SurfacedFactView>,
    pub injection_reason: String,
    pub injected_chars: usize,
}

/// The framed text of a surfaced fact is deliberately excluded: it is large and it is already in
/// the ledger. `--full` carries the whole trace for anyone who needs it.
#[derive(Debug, Serialize)]
pub struct SurfacedFactView {
    pub title: String,
    pub url: String,
    pub source_domain: String,
    pub trust_tier: String,
}

#[derive(Debug, Serialize)]
pub struct TraceBundle {
    pub injection: Option<VolitionContextInjectionTrace>,
    pub formation: Option<LiveGoalFormationTrace>,
    pub initiative: Option<RealtimeBoundedInitiativeTrace>,
    pub world: Option<WorldConsultationTrace>,
    pub turn_context: Option<TurnContextView>,
    pub exchange: Exchange,
}

#[derive(Debug, Serialize)]
pub struct TurnContextView {
    pub request_hash: String,
    pub messages: Vec<serde_json::Value>,
}

/// Renders one activation keyword as `term:weight_class`.
pub fn render_keyword(keyword: &ActivationKeyword) -> String {
    let class = match keyword.weight_class {
        KeywordWeightClass::Weak => "weak",
        KeywordWeightClass::Normal => "normal",
        KeywordWeightClass::Strong => "strong",
    };
    format!("{}:{}", keyword.term, class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_rendering_pairs_term_with_weight_class() {
        assert_eq!(
            render_keyword(&ActivationKeyword::strong("evidence")),
            "evidence:strong"
        );
        assert_eq!(render_keyword(&ActivationKeyword::weak("i")), "i:weak");
    }

    #[test]
    fn absent_sections_serialize_as_null_so_keys_stay_stable() {
        let line = TranscriptLine::Turn(TurnLine {
            turn: 0,
            at: None,
            user: "hello".to_string(),
            assistant: None,
            status: "completed".to_string(),
            volition: None,
            initiative: None,
            formation: None,
            world: None,
            undecodable: vec![],
            traces: None,
        });

        let json = serde_json::to_string(&line).expect("serialize");

        assert!(json.contains(r#""kind":"turn""#));
        assert!(json.contains(r#""volition":null"#));
        assert!(json.contains(r#""formation":null"#));
        assert!(
            !json.contains("traces"),
            "traces must be omitted entirely outside --full"
        );
    }

    #[test]
    fn the_curated_view_serializes_no_floating_point() {
        // Pins the integer-only guarantee for the default output. `--full` is exempt by design; see
        // Global Constraints.
        let line = TranscriptLine::Turn(TurnLine {
            turn: 0,
            at: None,
            user: "hello".to_string(),
            assistant: None,
            status: "completed".to_string(),
            volition: Some(VolitionView {
                threshold: 4,
                mode: Some(Mode::Neutral),
                winner: Some(WinnerView {
                    goal: "g".to_string(),
                    title: "G".to_string(),
                    effective_tier: 1,
                    biased_tier: 1,
                    losers: 2,
                }),
                fired: vec![MatchView {
                    goal: "g".to_string(),
                    strength: 9,
                    keywords: vec!["remember:normal".to_string()],
                    visibility: GoalVisibility::Conscious,
                    reason: None,
                }],
                below_threshold: vec![],
                omitted_count: 3,
                suppressed_cooldown_count: 0,
                blocked_count: 0,
                subconscious_selected_count: 0,
            }),
            initiative: None,
            formation: None,
            world: None,
            undecodable: vec![],
            traces: None,
        });

        let value = serde_json::to_value(&line).expect("serialize");
        assert!(
            !contains_float(&value),
            "the curated view must not emit floating point: {value}"
        );
    }

    fn contains_float(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(number) => number.is_f64(),
            serde_json::Value::Array(items) => items.iter().any(contains_float),
            serde_json::Value::Object(fields) => fields.values().any(contains_float),
            _ => false,
        }
    }

    #[test]
    fn a_complete_run_says_so_in_the_serialized_header() {
        let line = TranscriptLine::Session(SessionLine {
            session_id: "s".to_string(),
            ledger: "ledger.jsonl".to_string(),
            run_index: 1,
            run_started_at: None,
            turn_count: 1,
            source: SourceIntegrity {
                complete: true,
                skipped_line_count: 0,
                skipped_lines: vec![],
                orphans: OrphanCounts::default(),
            },
        });

        let json = serde_json::to_string(&line).expect("serialize");

        assert!(json.contains(r#""complete":true"#));
        assert!(json.contains(r#""skipped_line_count":0"#));
    }
}
