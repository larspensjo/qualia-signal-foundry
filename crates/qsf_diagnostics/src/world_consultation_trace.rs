use serde::{Deserialize, Serialize};

use qsf_corpus::QueryCandidate;
use qsf_volition::{InitiativeOutput, WorldQueryTerm};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldInjectionPoint {
    InlineSameTurn,
    DeferredNextTurn,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateEligibility {
    Eligible,
    Omitted { reason: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldConsultationCandidate {
    #[serde(flatten)]
    pub candidate: QueryCandidate,
    pub eligibility: CandidateEligibility,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SurfacedWorldFact {
    pub content_hash: String,
    pub title: String,
    pub url: String,
    pub source_domain: String,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_utc: time::OffsetDateTime,
    pub trust_tier: String,
    /// Exact model-visible material for this external source, including its sandbox wrapper.
    pub framed_text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CorpusMarkerMetadata {
    pub schema_version: u32,
    pub producer: String,
    pub articles_indexed: usize,
    pub drift_warning: Option<String>,
    pub corpus_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldEffectBoundary {
    pub initiative_output: InitiativeOutput,
    pub external_effect_executed: bool,
}

/// The topic-term requirement applied to a goal-activation lookup. `required_matches` is
/// calculated from `total_terms` using `WORLD_CONSULT_TOPIC_TERM_MINIMUM_MATCH_PERCENT`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TopicTermMajorityThreshold {
    pub required_matches: usize,
    pub total_terms: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldConsultationTrace {
    pub serving_goal_id: String,
    pub serving_goal_title: String,
    pub serving_tension_ids: Vec<String>,
    pub query_terms: Vec<WorldQueryTerm>,
    /// Every surfaced candidate matched every item in this relevance gate.
    pub required_anchors: Vec<String>,
    /// The subset of `required_anchors` derived from serving-goal activation terms.
    pub goal_derived_required_anchors: Vec<String>,
    /// Present only for the goal-activation policy; explicit entity/version requests retain
    /// their existing anchor-only relevance gate.
    pub topic_term_majority_threshold: Option<TopicTermMajorityThreshold>,
    pub candidates: Vec<WorldConsultationCandidate>,
    pub surfaced_facts: Vec<SurfacedWorldFact>,
    pub injected_text: String,
    pub lookup_latency_ms: u64,
    pub lookup_latency_ns: u64,
    pub injection_point: WorldInjectionPoint,
    pub injection_reason: String,
    pub corpus_marker: CorpusMarkerMetadata,
    pub bounded_or_external_output: WorldEffectBoundary,
    pub response_create_event_ref: String,
    pub artifact_or_record_reference: String,
}
