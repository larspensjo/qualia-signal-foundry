use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::time::Instant;

use serde::Serialize;
use time::OffsetDateTime;

use super::association::{Association, ensure_current_association_schema};
use super::judge_admission::{
    AdmissionBasis, JudgeVerdict, MAX_JUDGE_VERDICT_BASIS_POINTS, SelectionEligibility,
};
use super::record::{MemoryProvenance, MemoryRecord, ensure_current_memory_schema};

pub const DECAY_HALFLIFE_DAYS: f64 = 30.0;
/// Provisional half-life for external world observations, pending evidence from
/// the sleep world-memory consolidation experiment.
pub const WORLD_OBSERVATION_DECAY_HALFLIFE_DAYS: f64 = 7.0;
pub const RELEVANCE_GATE_SKIP_REASON: &str =
    "relevance gate: no keyword, tag, association, or profile signal";
pub const JUDGE_VERDICT_BELOW_ADMISSION_THRESHOLD_SKIP_REASON: &str =
    "judge verdict below admission threshold";
pub const RETRIEVAL_LIMIT_SKIP_REASON: &str = "retrieval limit exceeded";
pub const SUPERSEDED_WORLD_OBSERVATION_SKIP_REASON: &str = "superseded by newer world observation";
/// Starting recall-leaning memory admission threshold; tune on the development split.
/// A model-version or question-wording change invalidates this operating point.
pub const MEMORY_JUDGE_ADMISSION_THRESHOLD_BASIS_POINTS: u16 = 3_000;
/// Maximum judge addition to a retrieval total for an admitted verdict, reached at 10000 basis points.
/// Below-threshold verdicts, abstentions, and missing verdicts add nothing.
pub const MAX_JUDGE_SCORE_ADDITION: f64 = 2.0;
/// Reserved policy share of the retrieval limit filled from judge-ranked candidates first.
/// Fractional slot counts round up to the next whole retrieval slot.
pub const RESERVED_JUDGE_SLOT_SHARE: f64 = 0.25;

/// How a judged candidate's selection priority is combined with lexical retrieval.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionCombinationPolicy {
    /// Adds a bounded judge component only for verdicts at or above the admission threshold.
    /// This is the provisional code default; the development split chooses the winner.
    #[default]
    BoundedAdditive,
    /// Fills lexical slots first, then reserves a share for judge-admitted candidates
    /// outside those slots; unused reserved slots return to lexical order.
    ReservedSlots,
}

/// Provisional combination-policy default, pending the development-split comparison.
pub const DEFAULT_ADMISSION_COMBINATION_POLICY: AdmissionCombinationPolicy =
    AdmissionCombinationPolicy::BoundedAdditive;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalStrategy {
    RecencyOnly,
    KeywordTag,
    AssociationWeighted,
}

/// Extension rule: `new` takes only always-required inputs; optional signals arrive through `with_*` methods.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct RetrievalRequest<'a> {
    pub records: &'a [MemoryRecord],
    pub associations: &'a [Association],
    pub query: &'a str,
    pub strategy: RetrievalStrategy,
    pub limit: usize,
    pub evaluation_time: OffsetDateTime,
    judge_verdicts: Option<BTreeMap<String, JudgeVerdict>>,
    admission_threshold_basis_points: u16,
    combination_policy: AdmissionCombinationPolicy,
}

impl<'a> RetrievalRequest<'a> {
    pub fn new(
        records: &'a [MemoryRecord],
        associations: &'a [Association],
        query: &'a str,
        strategy: RetrievalStrategy,
        limit: usize,
        evaluation_time: OffsetDateTime,
    ) -> Self {
        Self {
            records,
            associations,
            query,
            strategy,
            limit,
            evaluation_time,
            judge_verdicts: None,
            admission_threshold_basis_points: MEMORY_JUDGE_ADMISSION_THRESHOLD_BASIS_POINTS,
            combination_policy: DEFAULT_ADMISSION_COMBINATION_POLICY,
        }
    }

    /// Adds optional memory-keyed judge verdicts to this retrieval request.
    pub fn with_judge_verdicts(mut self, verdicts: BTreeMap<String, JudgeVerdict>) -> Self {
        self.judge_verdicts = Some(verdicts);
        self
    }

    /// Returns judge verdicts and their backend identity for caller-side recording.
    pub fn judge_verdicts(&self) -> Option<&BTreeMap<String, JudgeVerdict>> {
        self.judge_verdicts.as_ref()
    }

    /// Overrides the per-request judge admission threshold in basis points.
    pub fn with_admission_threshold_basis_points(mut self, threshold: u16) -> Self {
        self.admission_threshold_basis_points = threshold;
        self
    }

    /// Selects how judge influence is combined with lexical ordering.
    pub fn with_combination_policy(mut self, policy: AdmissionCombinationPolicy) -> Self {
        self.combination_policy = policy;
        self
    }
}

impl fmt::Display for RetrievalStrategy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecencyOnly => formatter.write_str("recency-only"),
            Self::KeywordTag => formatter.write_str("keyword-tag"),
            Self::AssociationWeighted => formatter.write_str("association-weighted"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RetrievalScore {
    pub total: f64,
    pub recency: f64,
    pub keyword: f64,
    pub tag: f64,
    pub association: f64,
    pub importance: f64,
    pub reinforcement: f64,
    pub judge: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssociationPath {
    pub from_memory_id: String,
    pub to_memory_id: String,
    pub weight: f64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RetrievedMemory {
    pub memory: MemoryRecord,
    pub strategy: RetrievalStrategy,
    pub score: RetrievalScore,
    /// Original verdict probability, including below-threshold scores and reserved-slot runs.
    pub judge_verdict_basis_points: Option<u16>,
    pub admission_basis: AdmissionBasis,
    pub selection_eligibility: SelectionEligibility,
    pub matched_terms: Vec<String>,
    pub association_paths: Vec<AssociationPath>,
    pub skip_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RetrievalResult {
    pub query: String,
    pub strategy: RetrievalStrategy,
    pub evaluation_time: OffsetDateTime,
    pub combination_policy: AdmissionCombinationPolicy,
    pub judge_verdicts_supplied: bool,
    pub lexical_only_selected_ids: Vec<String>,
    pub selected: Vec<RetrievedMemory>,
    pub omitted: Vec<RetrievedMemory>,
    pub latency_ms: u64,
    pub latency_ns: u64,
}

impl RetrievalResult {
    /// Returns the retrieval ordering when assembly must preserve reserved judge slots.
    pub fn context_ordering(&self) -> Option<Vec<String>> {
        (self.combination_policy == AdmissionCombinationPolicy::ReservedSlots
            && self.judge_verdicts_supplied)
            .then(|| retrieved_memory_ids(&self.selected))
    }
}

pub fn retrieve_memories(request: &RetrievalRequest<'_>) -> anyhow::Result<RetrievalResult> {
    anyhow::ensure!(
        request.admission_threshold_basis_points <= MAX_JUDGE_VERDICT_BASIS_POINTS,
        "admission threshold must be in 0..=10000 basis points"
    );
    if let Some(verdicts) = &request.judge_verdicts {
        let mut identity = None;
        for (memory_id, verdict) in verdicts {
            anyhow::ensure!(
                verdict.score_basis_points <= MAX_JUDGE_VERDICT_BASIS_POINTS,
                "judge verdict for `{memory_id}` is outside 0..=10000 basis points"
            );
            anyhow::ensure!(
                verdict.model_identity.backend == verdict.backend_kind,
                "judge verdict for `{memory_id}` has inconsistent backend identity"
            );
            let operating_point = (
                &verdict.model_identity,
                &verdict.question_wording_version,
                verdict.backend_kind,
            );
            if let Some(first) = identity {
                anyhow::ensure!(
                    first == operating_point,
                    "judge verdicts mix operating-point identities"
                );
            } else {
                identity = Some(operating_point);
            }
        }
    }
    ensure_current_memory_schema(request.records)?;
    ensure_current_association_schema(request.associations)?;

    let started_at = Instant::now();
    let has_judge_verdicts = request
        .judge_verdicts
        .as_ref()
        .is_some_and(|verdicts| !verdicts.is_empty());
    let lexical_only = if has_judge_verdicts {
        Some(retrieve_selection(request, false))
    } else {
        None
    };
    let judged_selection = retrieve_selection(request, has_judge_verdicts);
    let mut selected = judged_selection.selected;
    let omitted = judged_selection.omitted;
    let lexical_only_selected_ids = lexical_only
        .as_ref()
        .map(|selection| retrieved_memory_ids(&selection.selected))
        .unwrap_or_else(|| retrieved_memory_ids(&selected));
    let lexical_ids = lexical_only_selected_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    for memory in &mut selected {
        memory.selection_eligibility = if lexical_ids.contains(&memory.memory.id) {
            SelectionEligibility::Associable
        } else {
            SelectionEligibility::JudgeInfluenced
        };
    }
    // Eligibility describes a selected memory only. Omitted entries never enter
    // the durable-structure consumer path.

    let elapsed = started_at.elapsed();

    Ok(RetrievalResult {
        query: request.query.to_string(),
        strategy: request.strategy,
        evaluation_time: request.evaluation_time,
        combination_policy: request.combination_policy,
        judge_verdicts_supplied: has_judge_verdicts,
        lexical_only_selected_ids,
        selected,
        omitted,
        latency_ms: duration_ms(elapsed),
        latency_ns: duration_ns(elapsed),
    })
}

struct CandidateForSelection {
    retrieved: RetrievedMemory,
    lexical_total: f64,
    judge_basis_points: Option<u16>,
}

struct RetrievalSelection {
    selected: Vec<RetrievedMemory>,
    omitted: Vec<RetrievedMemory>,
}

struct RelevanceSignals<'a> {
    score: &'a RetrievalScore,
    matched_terms: &'a [String],
    association_paths: &'a [AssociationPath],
    query: &'a str,
    query_terms: &'a HashSet<String>,
    judge_basis_points: Option<u16>,
    admission_threshold_basis_points: u16,
}

#[derive(Clone, Copy)]
struct JudgeScoring {
    basis_points: Option<u16>,
    admission_threshold_basis_points: u16,
    combination_policy: AdmissionCombinationPolicy,
}

fn retrieve_selection(request: &RetrievalRequest<'_>, include_judge: bool) -> RetrievalSelection {
    let query_terms = tokenize(request.query);
    let seed_ids = keyword_seed_ids(request.records, &query_terms);
    let association_paths = association_paths_by_target(request.associations, &seed_ids);
    let mut candidates = request
        .records
        .iter()
        .map(|record| {
            let matched_terms = matched_terms(record, &query_terms);
            let paths = association_paths
                .get(&record.id)
                .cloned()
                .unwrap_or_default();
            let verdict = include_judge
                .then(|| {
                    request
                        .judge_verdicts
                        .as_ref()
                        .and_then(|verdicts| verdicts.get(&record.id))
                })
                .flatten();
            let judge_basis_points = verdict.map(|verdict| verdict.score_basis_points);
            let (score, lexical_total) = score_record(
                record,
                request.strategy,
                request.evaluation_time,
                &matched_terms,
                &paths,
                JudgeScoring {
                    basis_points: judge_basis_points,
                    admission_threshold_basis_points: request.admission_threshold_basis_points,
                    combination_policy: request.combination_policy,
                },
            );
            CandidateForSelection {
                retrieved: RetrievedMemory {
                    memory: record.clone(),
                    strategy: request.strategy,
                    score,
                    judge_verdict_basis_points: judge_basis_points,
                    admission_basis: AdmissionBasis::Lexical,
                    selection_eligibility: SelectionEligibility::NotSelected,
                    matched_terms,
                    association_paths: paths,
                    skip_reason: None,
                },
                lexical_total,
                judge_basis_points,
            }
        })
        .collect::<Vec<_>>();

    candidates.sort_by(compare_candidates_by_total);

    let mut selected = Vec::new();
    let mut omitted = Vec::new();
    if include_judge && request.combination_policy == AdmissionCombinationPolicy::ReservedSlots {
        let mut eligible = Vec::new();
        for mut candidate in candidates {
            if admit_candidate(&mut candidate, request, &query_terms, true) {
                eligible.push(candidate);
            } else {
                omitted.push(candidate.retrieved);
            }
        }
        order_reserved_candidates(
            &mut eligible,
            request.limit,
            request.admission_threshold_basis_points,
        );
        apply_retrieval_limit(eligible, request.limit, &mut selected, &mut omitted);
    } else {
        for mut candidate in candidates {
            if !admit_candidate(&mut candidate, request, &query_terms, include_judge) {
                omitted.push(candidate.retrieved);
                continue;
            }
            if selected.len() < request.limit {
                candidate.retrieved.skip_reason = None;
                selected.push(candidate.retrieved);
            } else {
                candidate.retrieved.skip_reason = Some(RETRIEVAL_LIMIT_SKIP_REASON.to_string());
                omitted.push(candidate.retrieved);
            }
        }
    }

    RetrievalSelection { selected, omitted }
}

fn admit_candidate(
    candidate: &mut CandidateForSelection,
    request: &RetrievalRequest<'_>,
    query_terms: &HashSet<String>,
    include_judge: bool,
) -> bool {
    if is_superseded_world_observation(&candidate.retrieved.memory) {
        candidate.retrieved.skip_reason =
            Some(SUPERSEDED_WORLD_OBSERVATION_SKIP_REASON.to_string());
        return false;
    }

    let judge_basis_points = include_judge
        .then_some(candidate.judge_basis_points)
        .flatten();
    let (lexical_admitted, judge_admitted) = is_relevant_for_strategy(
        &candidate.retrieved.memory,
        request.strategy,
        RelevanceSignals {
            score: &candidate.retrieved.score,
            matched_terms: &candidate.retrieved.matched_terms,
            association_paths: &candidate.retrieved.association_paths,
            query: request.query,
            query_terms,
            judge_basis_points,
            admission_threshold_basis_points: request.admission_threshold_basis_points,
        },
    );
    candidate.retrieved.admission_basis = match (lexical_admitted, judge_admitted) {
        (true, true) => AdmissionBasis::LexicalAndJudge,
        (false, true) => AdmissionBasis::Judge,
        _ => AdmissionBasis::Lexical,
    };
    if !lexical_admitted && !judge_admitted {
        candidate.retrieved.skip_reason =
            Some(if include_judge && candidate.judge_basis_points.is_some() {
                JUDGE_VERDICT_BELOW_ADMISSION_THRESHOLD_SKIP_REASON.to_string()
            } else {
                RELEVANCE_GATE_SKIP_REASON.to_string()
            });
        return false;
    }

    true
}

fn apply_retrieval_limit(
    eligible: Vec<CandidateForSelection>,
    limit: usize,
    selected: &mut Vec<RetrievedMemory>,
    omitted: &mut Vec<RetrievedMemory>,
) {
    for mut candidate in eligible {
        if selected.len() < limit {
            candidate.retrieved.skip_reason = None;
            selected.push(candidate.retrieved);
        } else {
            candidate.retrieved.skip_reason = Some(RETRIEVAL_LIMIT_SKIP_REASON.to_string());
            omitted.push(candidate.retrieved);
        }
    }
}

fn compare_candidates_by_total(
    left: &CandidateForSelection,
    right: &CandidateForSelection,
) -> std::cmp::Ordering {
    right
        .retrieved
        .score
        .total
        .total_cmp(&left.retrieved.score.total)
        .then_with(|| compare_candidates_by_lexical_total(left, right))
}

fn compare_candidates_by_lexical_total(
    left: &CandidateForSelection,
    right: &CandidateForSelection,
) -> std::cmp::Ordering {
    right
        .lexical_total
        .total_cmp(&left.lexical_total)
        .then_with(|| {
            right
                .retrieved
                .memory
                .created_at
                .cmp(&left.retrieved.memory.created_at)
        })
        .then_with(|| left.retrieved.memory.id.cmp(&right.retrieved.memory.id))
}

fn order_reserved_candidates(
    candidates: &mut Vec<CandidateForSelection>,
    limit: usize,
    admission_threshold_basis_points: u16,
) {
    let reserved_slots = ((limit as f64) * RESERVED_JUDGE_SLOT_SHARE).ceil() as usize;
    let lexical_slots = limit.saturating_sub(reserved_slots);
    let mut lexical_order = (0..candidates.len()).collect::<Vec<_>>();
    lexical_order.sort_by(|left, right| {
        compare_candidates_by_lexical_total(&candidates[*left], &candidates[*right])
    });
    let mut ordered_indices = lexical_order
        .iter()
        .take(lexical_slots)
        .copied()
        .collect::<Vec<_>>();
    let mut selected = ordered_indices.iter().copied().collect::<HashSet<_>>();
    let mut judge_ranked = (0..candidates.len())
        .filter(|index| {
            !selected.contains(index)
                && candidates[*index]
                    .judge_basis_points
                    .is_some_and(|points| points >= admission_threshold_basis_points)
        })
        .collect::<Vec<_>>();
    judge_ranked.sort_by(|left, right| {
        candidates[*right]
            .judge_basis_points
            .cmp(&candidates[*left].judge_basis_points)
            .then_with(|| {
                compare_candidates_by_lexical_total(&candidates[*left], &candidates[*right])
            })
    });

    for index in judge_ranked.into_iter().take(reserved_slots) {
        selected.insert(index);
        ordered_indices.push(index);
    }
    ordered_indices.extend(
        lexical_order
            .into_iter()
            .filter(|index| !selected.contains(index)),
    );

    let mut indexed = candidates.drain(..).map(Some).collect::<Vec<_>>();
    *candidates = ordered_indices
        .into_iter()
        .map(|index| indexed[index].take().expect("candidate index is unique"))
        .collect();
}

pub fn retrieved_memory_ids(memories: &[RetrievedMemory]) -> Vec<String> {
    memories
        .iter()
        .map(|memory| memory.memory.id.clone())
        .collect()
}

fn score_record(
    record: &MemoryRecord,
    strategy: RetrievalStrategy,
    evaluation_time: OffsetDateTime,
    matched_terms: &[String],
    association_paths: &[AssociationPath],
    judge_scoring: JudgeScoring,
) -> (RetrievalScore, f64) {
    let recency = compute_recency_decay(record, evaluation_time);
    let keyword = matched_terms_in_text(record, matched_terms) as f64;
    let tag = matched_terms_in_tags(record, matched_terms) as f64;
    let association = association_paths
        .iter()
        .map(|path| path.weight)
        .sum::<f64>()
        .min(2.0);
    let importance = record.importance;
    let reinforcement = f64::from(record.reinforcement_count).min(5.0) / 5.0;

    let lexical_total = match strategy {
        RetrievalStrategy::RecencyOnly => recency,
        RetrievalStrategy::KeywordTag => {
            (keyword * 0.8) + (tag * 1.4) + (importance * 0.35) + (recency * 0.2)
        }
        RetrievalStrategy::AssociationWeighted => {
            (keyword * 0.65)
                + (tag * 1.1)
                + (association * 1.35)
                + (importance * 0.35)
                + (recency * 0.2)
                + (reinforcement * 0.25)
        }
    };
    let judge = match (judge_scoring.combination_policy, judge_scoring.basis_points) {
        (AdmissionCombinationPolicy::BoundedAdditive, Some(points))
            if points >= judge_scoring.admission_threshold_basis_points =>
        {
            (f64::from(points) / f64::from(MAX_JUDGE_VERDICT_BASIS_POINTS))
                * MAX_JUDGE_SCORE_ADDITION
        }
        _ => 0.0,
    };
    let total = lexical_total + judge;

    (
        RetrievalScore {
            total,
            recency,
            keyword,
            tag,
            association,
            importance,
            reinforcement,
            judge,
        },
        lexical_total,
    )
}

fn is_relevant_for_strategy(
    record: &MemoryRecord,
    strategy: RetrievalStrategy,
    signals: RelevanceSignals<'_>,
) -> (bool, bool) {
    let lexical = match strategy {
        RetrievalStrategy::RecencyOnly => true,
        RetrievalStrategy::KeywordTag => {
            has_direct_relevance(
                record,
                signals.score,
                signals.matched_terms,
                signals.query,
                signals.query_terms,
            ) || profile_identity_allowed(record, signals.query, signals.query_terms)
        }
        RetrievalStrategy::AssociationWeighted => {
            has_direct_relevance(
                record,
                signals.score,
                signals.matched_terms,
                signals.query,
                signals.query_terms,
            ) || !signals.association_paths.is_empty()
                || profile_identity_allowed(record, signals.query, signals.query_terms)
        }
    };
    let judge = signals
        .judge_basis_points
        .is_some_and(|score| score >= signals.admission_threshold_basis_points);
    (lexical, judge)
}

fn has_direct_relevance(
    record: &MemoryRecord,
    score: &RetrievalScore,
    matched_terms: &[String],
    query: &str,
    query_terms: &HashSet<String>,
) -> bool {
    if score.keyword <= 0.0 && score.tag <= 0.0 {
        return false;
    }

    if memory_has_identity_or_profile_tag(record)
        && matched_terms
            .iter()
            .all(|term| is_generic_identity_term(term))
    {
        return identity_query_target(query, query_terms)
            .map(|target| record_matches_identity_target(record, target))
            .unwrap_or(false);
    }

    true
}

fn profile_identity_allowed(
    record: &MemoryRecord,
    query: &str,
    query_terms: &HashSet<String>,
) -> bool {
    identity_query_target(query, query_terms)
        .map(|target| record_matches_identity_target(record, target))
        .unwrap_or(false)
}

fn memory_has_identity_or_profile_tag(record: &MemoryRecord) -> bool {
    record.tags.iter().any(|tag| {
        let tag = tag.to_ascii_lowercase();
        tag == "profile"
            || tag == "identity"
            || tag.ends_with("_identity")
            || tag.ends_with("-identity")
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IdentityTarget {
    Assistant,
    User,
    Any,
}

fn is_generic_identity_term(term: &str) -> bool {
    matches!(
        term,
        "assistant" | "called" | "identity" | "name" | "profile" | "user" | "you" | "your"
    )
}

fn identity_query_target(query: &str, query_terms: &HashSet<String>) -> Option<IdentityTarget> {
    let normalized = normalize_query_for_phrase_match(query);
    if normalized.contains(" who are you ")
        || normalized.contains(" what is your name ")
        || normalized.contains(" what s your name ")
        || normalized.contains(" whats your name ")
        || normalized.contains(" what should i call you ")
        || normalized.contains(" what are you called ")
        || (query_terms.contains("assistant") && query_terms.contains("name"))
    {
        return Some(IdentityTarget::Assistant);
    }

    if normalized.contains(" who am i ")
        || normalized.contains(" what is my name ")
        || normalized.contains(" what s my name ")
        || normalized.contains(" whats my name ")
        || normalized.contains(" what should you call me ")
        || normalized.contains(" what am i called ")
        || (query_terms.contains("user") && query_terms.contains("name"))
    {
        return Some(IdentityTarget::User);
    }

    if query_terms.contains("name") && (query_terms.contains("what") || query_terms.contains("who"))
    {
        if query_terms.contains("your")
            || query_terms.contains("you")
            || query_terms.contains("assistant")
        {
            return Some(IdentityTarget::Assistant);
        }

        if normalized.contains(" my ") || normalized.contains(" me ") || normalized.contains(" i ")
        {
            return Some(IdentityTarget::User);
        }
    }

    if query_terms.contains("who")
        && query_terms.contains("name")
        && query_terms.contains("assistant")
    {
        return Some(IdentityTarget::Assistant);
    }

    if query_terms.contains("who") && query_terms.contains("name") && query_terms.contains("user") {
        return Some(IdentityTarget::User);
    }

    None
}

fn record_matches_identity_target(record: &MemoryRecord, target: IdentityTarget) -> bool {
    match record_identity_target(record) {
        IdentityTarget::Any => false,
        record_target => record_target == target,
    }
}

fn record_identity_target(record: &MemoryRecord) -> IdentityTarget {
    if record
        .tags
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case("assistant_identity"))
        || record
            .title
            .to_ascii_lowercase()
            .starts_with("assistant name:")
    {
        IdentityTarget::Assistant
    } else if record
        .tags
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case("user_identity"))
        || record.title.to_ascii_lowercase().starts_with("user name:")
    {
        IdentityTarget::User
    } else {
        IdentityTarget::Any
    }
}

fn normalize_query_for_phrase_match(query: &str) -> String {
    let collapsed = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    format!(" {collapsed} ")
}

pub(crate) fn compute_recency_decay(record: &MemoryRecord, evaluation_time: OffsetDateTime) -> f64 {
    let reference = record.last_reinforced_at.unwrap_or(record.created_at);
    let age_seconds = (evaluation_time - reference).whole_seconds().max(0) as f64;
    let age_days = age_seconds / 86_400.0;
    (-std::f64::consts::LN_2 * age_days / effective_decay_halflife_days(record)).exp()
}

fn effective_decay_halflife_days(record: &MemoryRecord) -> f64 {
    record
        .time_sensitive_decay_half_life_days
        .unwrap_or_else(|| {
            if record.provenance == MemoryProvenance::WorldObservationExternal {
                WORLD_OBSERVATION_DECAY_HALFLIFE_DAYS
            } else {
                DECAY_HALFLIFE_DAYS
            }
        })
}

fn is_superseded_world_observation(record: &MemoryRecord) -> bool {
    record.provenance == MemoryProvenance::WorldObservationExternal
        && record.superseded_by.is_some()
}

fn tokenize(input: &str) -> HashSet<String> {
    input
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let normalized = term.trim().to_ascii_lowercase();
            if normalized.len() < 3 {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn memory_terms(record: &MemoryRecord) -> HashSet<String> {
    let mut terms = tokenize(&record.title);
    terms.extend(tokenize(&record.summary));
    terms.extend(record.tags.iter().map(|tag| tag.to_ascii_lowercase()));
    terms
}

fn matched_terms(record: &MemoryRecord, query_terms: &HashSet<String>) -> Vec<String> {
    let memory_terms = memory_terms(record);
    let mut matches = query_terms
        .iter()
        .filter(|term| memory_terms.contains(*term))
        .cloned()
        .collect::<Vec<_>>();
    matches.sort();
    matches
}

fn matched_terms_in_text(record: &MemoryRecord, terms: &[String]) -> usize {
    let text_terms = tokenize(&format!("{} {}", record.title, record.summary));
    terms
        .iter()
        .filter(|term| text_terms.contains(term.as_str()))
        .count()
}

fn matched_terms_in_tags(record: &MemoryRecord, terms: &[String]) -> usize {
    let tags = record
        .tags
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    terms
        .iter()
        .filter(|term| tags.contains(term.as_str()))
        .count()
}

fn keyword_seed_ids(records: &[MemoryRecord], query_terms: &HashSet<String>) -> HashSet<String> {
    records
        .iter()
        .filter(|record| !matched_terms(record, query_terms).is_empty())
        .map(|record| record.id.clone())
        .collect()
}

fn association_paths_by_target(
    associations: &[Association],
    seed_ids: &HashSet<String>,
) -> HashMap<String, Vec<AssociationPath>> {
    let mut paths: HashMap<String, Vec<AssociationPath>> = HashMap::new();

    for association in associations {
        if seed_ids.contains(&association.from_memory_id) {
            paths
                .entry(association.to_memory_id.clone())
                .or_default()
                .push(AssociationPath {
                    from_memory_id: association.from_memory_id.clone(),
                    to_memory_id: association.to_memory_id.clone(),
                    weight: association.weight,
                    reason: association.reason.clone(),
                });
        }
    }

    for target_paths in paths.values_mut() {
        target_paths.sort_by(|left, right| right.weight.total_cmp(&left.weight));
    }

    paths
}

fn duration_ms(elapsed: std::time::Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

fn duration_ns(elapsed: std::time::Duration) -> u64 {
    u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        AdmissionCombinationPolicy, JUDGE_VERDICT_BELOW_ADMISSION_THRESHOLD_SKIP_REASON,
        RELEVANCE_GATE_SKIP_REASON, RetrievalRequest, RetrievalStrategy,
        SUPERSEDED_WORLD_OBSERVATION_SKIP_REASON, retrieve_memories,
    };
    use crate::record::{MemoryProvenance, MemoryRecord, MemoryRecordKind, MemoryTrustTier};
    use crate::{
        AdmissionBasis, Association, JudgeVerdict, MEMORY_JUDGE_ADMISSION_THRESHOLD_BASIS_POINTS,
        SelectionEligibility,
    };
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    #[test]
    fn recency_only_prefers_newest_records() {
        let records = vec![
            test_record(
                "memory.one",
                "Recent",
                "Recent",
                vec![],
                OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap(),
            ),
            test_record(
                "memory.two",
                "Older",
                "Older",
                vec![],
                OffsetDateTime::parse("2026-05-01T00:00:00Z", &Rfc3339).unwrap(),
            ),
        ];

        let result = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "context memory",
            RetrievalStrategy::RecencyOnly,
            1,
            evaluation_time(),
        ))
        .unwrap();

        assert_eq!(result.selected[0].memory.id, "memory.one");
    }

    #[test]
    fn keyword_tag_retrieval_prefers_direct_matches() {
        let records = vec![
            test_record(
                "memory.context",
                "Context budget",
                "The live loop should select compact context.",
                vec!["context", "budget"],
                OffsetDateTime::parse("2026-05-25T00:00:00Z", &Rfc3339).unwrap(),
            ),
            test_record(
                "memory.retrieval",
                "Recall quality",
                "Useful memories should be inspectable.",
                vec!["retrieval"],
                OffsetDateTime::parse("2026-05-24T00:00:00Z", &Rfc3339).unwrap(),
            ),
        ];

        let result = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "retrieval",
            RetrievalStrategy::KeywordTag,
            1,
            evaluation_time(),
        ))
        .unwrap();

        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].memory.id, "memory.retrieval");
    }

    #[test]
    fn association_weighted_retrieval_includes_linked_memories() {
        let records = vec![
            test_record(
                "memory.context-budget",
                "Context budget",
                "The live loop should select compact context.",
                vec!["context", "budget"],
                OffsetDateTime::parse("2026-05-25T00:00:00Z", &Rfc3339).unwrap(),
            ),
            test_record(
                "memory.linked",
                "Linked note",
                "This is connected by association.",
                vec!["linked"],
                OffsetDateTime::parse("2026-05-24T00:00:00Z", &Rfc3339).unwrap(),
            ),
        ];
        let associations = vec![Association::new(
            "memory.context-budget",
            "memory.linked",
            0.8,
            "linked for retrieval",
            OffsetDateTime::parse("2026-05-26T00:00:00Z", &Rfc3339).unwrap(),
        )];

        let result = retrieve_memories(&RetrievalRequest::new(
            &records,
            &associations,
            "context budget",
            RetrievalStrategy::AssociationWeighted,
            3,
            evaluation_time(),
        ))
        .unwrap();

        assert!(
            result
                .selected
                .iter()
                .any(|memory| memory.memory.id == "memory.linked")
        );
    }

    #[test]
    fn identity_queries_retrieve_targeted_identity_memories() {
        let records = vec![
            test_record(
                "memory.ari",
                "Assistant name: Ari",
                "The assistant accepted Ari.",
                vec!["assistant_identity", "profile", "name"],
                OffsetDateTime::parse("2026-05-25T00:00:00Z", &Rfc3339).unwrap(),
            ),
            test_record(
                "memory.lars",
                "User name: Lars",
                "The user's name is Lars.",
                vec!["user_identity", "profile", "name"],
                OffsetDateTime::parse("2026-05-25T00:00:00Z", &Rfc3339).unwrap(),
            ),
        ];

        let assistant = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "What's your name?",
            RetrievalStrategy::KeywordTag,
            8,
            evaluation_time(),
        ))
        .unwrap();
        let user = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "What's my name?",
            RetrievalStrategy::KeywordTag,
            8,
            evaluation_time(),
        ))
        .unwrap();

        assert_eq!(assistant.selected[0].memory.id, "memory.ari");
        assert_eq!(user.selected[0].memory.id, "memory.lars");
    }

    #[test]
    fn recency_decay_halves_at_configured_halflife() {
        let now = OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap();
        let record = MemoryRecord::new(
            "memory.halflife",
            MemoryRecordKind::Observation,
            "Half-life",
            "Half-life",
            vec![],
            now - time::Duration::days(super::DECAY_HALFLIFE_DAYS as i64),
            0.5,
            0,
            "tests",
            10,
        );

        let score = super::compute_recency_decay(&record, now);
        assert!((score - 0.5).abs() < 0.001, "half-life score was {score}");
    }

    #[test]
    fn world_observation_decays_faster_than_same_age_first_party_record() {
        let now = OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap();
        let created_at = now - time::Duration::days(14);
        let first_party = test_record(
            "memory.first-party",
            "AI news",
            "A first-party fact.",
            vec![],
            created_at,
        );
        let world_observation = test_record(
            "memory.world-observation",
            "AI news",
            "An external fact.",
            vec![],
            created_at,
        )
        .with_world_observation();

        assert!(
            super::compute_recency_decay(&world_observation, now)
                < super::compute_recency_decay(&first_party, now)
        );
    }

    #[test]
    fn frozen_evaluation_time_keeps_selection_deterministic_across_clock_changes() {
        let records = recency_ordering_records();
        let early = evaluation_time();
        let late = early + time::Duration::days(183);

        let first = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "topic",
            RetrievalStrategy::KeywordTag,
            1,
            early,
        ))
        .unwrap();
        let simulated_clock_change = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "topic",
            RetrievalStrategy::KeywordTag,
            1,
            late,
        ))
        .unwrap();
        let repeated = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "topic",
            RetrievalStrategy::KeywordTag,
            1,
            early,
        ))
        .unwrap();

        assert_eq!(first.evaluation_time, early);
        assert_eq!(first.selected, repeated.selected);
        assert_eq!(first.omitted, repeated.omitted);
        assert_ne!(first.selected, simulated_clock_change.selected);
    }

    #[test]
    fn different_evaluation_time_changes_recency_driven_ordering() {
        let records = recency_ordering_records();
        let early = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "topic",
            RetrievalStrategy::KeywordTag,
            1,
            evaluation_time(),
        ))
        .unwrap();
        let late = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "topic",
            RetrievalStrategy::KeywordTag,
            1,
            evaluation_time() + time::Duration::days(183),
        ))
        .unwrap();

        assert_eq!(early.selected[0].memory.id, "memory.newer");
        assert_eq!(late.selected[0].memory.id, "memory.older");
        assert_ne!(early.selected, late.selected);
    }

    #[test]
    fn superseded_world_observation_is_omitted_while_successor_is_retrieved() {
        let now = OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap();
        let superseded = test_record(
            "memory.world.old",
            "AI release",
            "The prior external observation.",
            vec![],
            now - time::Duration::days(1),
        )
        .with_world_observation()
        .with_superseded_by("memory.world.new");
        let successor = test_record(
            "memory.world.new",
            "AI release",
            "The newer external observation.",
            vec![],
            now,
        )
        .with_world_observation();

        let result = retrieve_memories(&RetrievalRequest::new(
            &[superseded, successor],
            &[],
            "AI release",
            RetrievalStrategy::KeywordTag,
            2,
            now,
        ))
        .unwrap();

        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].memory.id, "memory.world.new");
        assert_eq!(result.omitted.len(), 1);
        assert_eq!(result.omitted[0].memory.id, "memory.world.old");
        assert_eq!(
            result.omitted[0].skip_reason.as_deref(),
            Some(SUPERSEDED_WORLD_OBSERVATION_SKIP_REASON)
        );
    }

    #[test]
    fn legacy_record_defaults_and_retrieves_unchanged() {
        let legacy_json = r#"{
            "schema_version": 1,
            "id": "memory.legacy",
            "kind": "observation",
            "title": "Legacy retrieval",
            "summary": "A durable first-party memory.",
            "tags": ["legacy"],
            "created_at": "2026-05-09T12:00:00Z",
            "importance": 0.5,
            "reinforcement_count": 0,
            "source_reference": "tests",
            "estimated_tokens": 10
        }"#;
        let record: MemoryRecord = serde_json::from_str(legacy_json).unwrap();
        assert!(record.ensure_current_schema().is_ok());

        let result = retrieve_memories(&RetrievalRequest::new(
            &[record],
            &[],
            "legacy",
            RetrievalStrategy::KeywordTag,
            1,
            evaluation_time(),
        ))
        .unwrap();

        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].memory.id, "memory.legacy");
        assert_eq!(
            result.selected[0].memory.provenance,
            MemoryProvenance::FirstPartyInternal
        );
        assert_eq!(
            result.selected[0].memory.trust_tier,
            MemoryTrustTier::Trusted
        );
        assert_eq!(
            result.selected[0]
                .memory
                .time_sensitive_decay_half_life_days,
            None
        );
        assert_eq!(result.selected[0].memory.superseded_by, None);
    }

    #[test]
    fn supersession_marker_does_not_apply_to_first_party_records() {
        let record = test_record(
            "memory.first-party",
            "Project direction",
            "An internal decision remains retrievable.",
            vec![],
            OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap(),
        )
        .with_superseded_by("memory.unrelated");

        let result = retrieve_memories(&RetrievalRequest::new(
            &[record],
            &[],
            "project direction",
            RetrievalStrategy::KeywordTag,
            1,
            evaluation_time(),
        ))
        .unwrap();

        assert_eq!(result.selected.len(), 1);
        assert!(result.omitted.is_empty());
    }

    #[test]
    fn judge_admits_zero_signal_memories_and_reports_below_threshold_omissions() {
        let record = test_record(
            "memory.zero-signal",
            "Seed starting indoors",
            "A note about growing seedlings.",
            vec![],
            evaluation_time(),
        );
        let records = [record];
        let high = retrieve_memories(
            &RetrievalRequest::new(
                &records,
                &[],
                "the meaning of quiet winter light",
                RetrievalStrategy::KeywordTag,
                1,
                evaluation_time(),
            )
            .with_judge_verdicts(verdicts(&[("memory.zero-signal", 8_500)])),
        )
        .unwrap();

        assert_eq!(high.selected.len(), 1);
        assert_eq!(high.selected[0].memory.id, "memory.zero-signal");
        assert_eq!(high.selected[0].admission_basis, AdmissionBasis::Judge);
        assert!(high.selected[0].score.judge > 0.0);

        let low = retrieve_memories(
            &RetrievalRequest::new(
                &records,
                &[],
                "the meaning of quiet winter light",
                RetrievalStrategy::KeywordTag,
                1,
                evaluation_time(),
            )
            .with_judge_verdicts(verdicts(&[("memory.zero-signal", 2_999)])),
        )
        .unwrap();

        assert!(low.selected.is_empty());
        assert_eq!(
            low.omitted[0].skip_reason.as_deref(),
            Some(JUDGE_VERDICT_BELOW_ADMISSION_THRESHOLD_SKIP_REASON)
        );
        assert_eq!(MEMORY_JUDGE_ADMISSION_THRESHOLD_BASIS_POINTS, 3_000);
    }

    #[test]
    fn high_judge_verdict_does_not_resurrect_a_superseded_world_observation() {
        let record = test_record(
            "memory.superseded-world",
            "An old unrelated report",
            "An old external observation.",
            vec![],
            evaluation_time(),
        )
        .with_world_observation()
        .with_superseded_by("memory.world-successor");

        let result = retrieve_memories(
            &RetrievalRequest::new(
                &[record],
                &[],
                "quiet winter light",
                RetrievalStrategy::KeywordTag,
                1,
                evaluation_time(),
            )
            .with_judge_verdicts(verdicts(&[("memory.superseded-world", 10_000)])),
        )
        .unwrap();

        assert!(result.selected.is_empty());
        assert_eq!(
            result.omitted[0].skip_reason.as_deref(),
            Some(SUPERSEDED_WORLD_OBSERVATION_SKIP_REASON)
        );
    }

    #[test]
    fn abstained_pair_score_does_not_admit_a_zero_signal_memory() {
        use qsf_semantics::Traced;
        use qsf_semantics::trace::{SemanticFailure, SemanticOperation, SemanticTraceRecord};
        use qsf_semantics::{PairScore, RelevanceTask, ScoreKind};

        let record = test_record(
            "memory.abstained",
            "Seed starting indoors",
            "A note about growing seedlings.",
            vec![],
            evaluation_time(),
        );
        let pair_score = PairScore {
            candidate_id: "candidate.seed".to_owned(),
            candidate_content_hash: "seed-hash".to_owned(),
            score_basis_points: 10_000,
            score_kind: ScoreKind::Probability,
            abstained: true,
            abstain_reason: Some("uncertain".to_owned()),
        };
        let mut trace = SemanticTraceRecord::unavailable(
            RelevanceTask::MemoryRelevance,
            SemanticOperation::PairScore,
            SemanticFailure::BackendUnavailable {
                detail: "adapter fixture".to_owned(),
            },
        );
        trace.failure = None;
        trace.invocation_id = "fixture-success".to_owned();
        trace.model_identity.model_id = "fixture-judge".to_owned();
        trace.question_wording_version = "memory-relevance-v1".to_owned();
        let verdicts = crate::adapt_pair_scores_to_judge_verdicts(
            &Traced::success(trace, vec![pair_score]),
            &BTreeMap::from([("candidate.seed".to_owned(), "memory.abstained".to_owned())]),
        )
        .unwrap();
        let result = retrieve_memories(
            &RetrievalRequest::new(
                &[record],
                &[],
                "quiet winter light",
                RetrievalStrategy::KeywordTag,
                1,
                evaluation_time(),
            )
            .with_judge_verdicts(verdicts),
        )
        .unwrap();

        assert!(result.selected.is_empty());
        assert_eq!(
            result.omitted[0].skip_reason.as_deref(),
            Some(RELEVANCE_GATE_SKIP_REASON)
        );
    }

    #[test]
    fn no_verdict_selection_scores_and_omissions_match_under_both_policies() {
        let mut a = test_record("a", "orchid one", "", vec![], evaluation_time());
        let mut b = test_record("b", "orchid two", "", vec![], evaluation_time());
        let mut c = test_record("c", "orchid three", "", vec![], evaluation_time());
        let mut z = test_record("z", "unrelated", "", vec![], evaluation_time());
        a.importance = 0.0;
        b.importance = 0.0;
        c.importance = 0.0;
        z.importance = 1.0;
        let records = [a, b, c, z];
        let default_result = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "orchid",
            RetrievalStrategy::KeywordTag,
            2,
            evaluation_time(),
        ))
        .unwrap();
        assert_eq!(
            super::retrieved_memory_ids(&default_result.selected),
            ["a", "b"]
        );
        assert_eq!(
            super::retrieved_memory_ids(&default_result.omitted),
            ["c", "z"]
        );
        let default_selection =
            serde_json::to_vec(&(&default_result.selected, &default_result.omitted)).unwrap();
        let mut serialized_selection = None;

        for policy in [
            AdmissionCombinationPolicy::BoundedAdditive,
            AdmissionCombinationPolicy::ReservedSlots,
        ] {
            let result = retrieve_memories(
                &RetrievalRequest::new(
                    &records,
                    &[],
                    "orchid",
                    RetrievalStrategy::KeywordTag,
                    2,
                    evaluation_time(),
                )
                .with_combination_policy(policy),
            )
            .unwrap();
            let selected_ids = super::retrieved_memory_ids(&result.selected);
            let omitted_ids = super::retrieved_memory_ids(&result.omitted);
            assert_eq!(result.combination_policy, policy);
            assert!(!result.judge_verdicts_supplied);
            assert!(result.context_ordering().is_none());
            assert_eq!(selected_ids, ["a", "b"]);
            assert_eq!(omitted_ids, ["c", "z"]);
            assert_eq!(result.selected[0].score.total, 1.0);
            assert_eq!(result.selected[0].score.judge, 0.0);
            assert_eq!(
                result.omitted[0].skip_reason.as_deref(),
                Some(super::RETRIEVAL_LIMIT_SKIP_REASON)
            );
            assert_eq!(
                result.omitted[1].skip_reason.as_deref(),
                Some(RELEVANCE_GATE_SKIP_REASON)
            );
            let bytes = serde_json::to_vec(&(&result.selected, &result.omitted)).unwrap();
            assert_eq!(bytes, default_selection);
            if let Some(expected) = &serialized_selection {
                assert_eq!(&bytes, expected);
            } else {
                serialized_selection = Some(bytes);
            }
        }
    }

    #[test]
    fn reserved_slots_exclude_below_threshold_verdicts() {
        let records = ["a", "b", "c", "d", "e"].map(|id| {
            let mut record = test_record(id, "orchid", "", vec![], evaluation_time());
            record.importance = 0.0;
            record
        });
        let result = retrieve_memories(
            &RetrievalRequest::new(
                &records,
                &[],
                "orchid",
                RetrievalStrategy::KeywordTag,
                4,
                evaluation_time(),
            )
            .with_judge_verdicts(verdicts(&[("e", 1_000)]))
            .with_combination_policy(AdmissionCombinationPolicy::ReservedSlots),
        )
        .unwrap();
        assert_eq!(
            super::retrieved_memory_ids(&result.selected),
            ["a", "b", "c", "d"]
        );
        assert_eq!(result.lexical_only_selected_ids, ["a", "b", "c", "d"]);
        assert_eq!(
            result
                .omitted
                .iter()
                .find(|memory| memory.memory.id == "e")
                .unwrap()
                .judge_verdict_basis_points,
            Some(1_000)
        );
    }

    #[test]
    fn reserved_slot_goes_to_judge_admitted_candidate_outside_lexical_slots() {
        let mut records = ["a", "b", "c", "d"]
            .map(|id| {
                let mut record = test_record(id, "orchid", "", vec![], evaluation_time());
                record.importance = 0.0;
                record
            })
            .to_vec();
        let mut z = test_record("z", "unrelated", "", vec![], evaluation_time());
        z.importance = 0.0;
        records.push(z);
        let result = retrieve_memories(
            &RetrievalRequest::new(
                &records,
                &[],
                "orchid",
                RetrievalStrategy::KeywordTag,
                4,
                evaluation_time(),
            )
            .with_judge_verdicts(verdicts(&[("a", 9_500), ("z", 9_000)]))
            .with_combination_policy(AdmissionCombinationPolicy::ReservedSlots),
        )
        .unwrap();
        assert_eq!(
            super::retrieved_memory_ids(&result.selected),
            ["a", "b", "c", "z"]
        );
        assert_eq!(result.lexical_only_selected_ids, ["a", "b", "c", "d"]);
        let z = result
            .selected
            .iter()
            .find(|memory| memory.memory.id == "z")
            .unwrap();
        assert_eq!(z.score.total, 0.2);
        assert_eq!(z.score.judge, 0.0);
        assert_eq!(z.judge_verdict_basis_points, Some(9_000));
        assert_eq!(
            z.selection_eligibility,
            SelectionEligibility::JudgeInfluenced
        );
    }

    #[test]
    fn below_threshold_verdict_adds_no_score_or_priority() {
        let records = ["a", "b"].map(|id| {
            let mut record = test_record(id, "orchid", "", vec![], evaluation_time());
            record.importance = 0.0;
            record
        });
        let result = retrieve_memories(
            &RetrievalRequest::new(
                &records,
                &[],
                "orchid",
                RetrievalStrategy::KeywordTag,
                1,
                evaluation_time(),
            )
            .with_judge_verdicts(verdicts(&[("b", 1_000)])),
        )
        .unwrap();
        assert_eq!(super::retrieved_memory_ids(&result.selected), ["a"]);
        assert_eq!(result.omitted[0].score.judge, 0.0);
    }

    #[test]
    fn retrieval_rejects_invalid_threshold_verdict_and_mixed_identity() {
        let records = [test_record("a", "orchid", "", vec![], evaluation_time())];
        let request = || {
            RetrievalRequest::new(
                &records,
                &[],
                "orchid",
                RetrievalStrategy::KeywordTag,
                1,
                evaluation_time(),
            )
        };
        assert!(
            retrieve_memories(&request().with_admission_threshold_basis_points(10_001)).is_err()
        );
        assert!(
            retrieve_memories(&request().with_judge_verdicts(verdicts(&[("a", 10_001)]))).is_err()
        );
        let mut mixed = verdicts(&[("a", 9_000), ("b", 8_000)]);
        mixed.get_mut("b").unwrap().question_wording_version = "another-wording".to_owned();
        assert!(retrieve_memories(&request().with_judge_verdicts(mixed.clone())).is_err());
        mixed.get_mut("b").unwrap().question_wording_version = "memory-relevance-v1".to_owned();
        mixed.get_mut("b").unwrap().model_identity.identity_value = "other-model".to_owned();
        assert!(retrieve_memories(&request().with_judge_verdicts(mixed)).is_err());
        let mut inconsistent = verdicts(&[("a", 9_000)]);
        inconsistent.get_mut("a").unwrap().backend_kind =
            qsf_semantics::trace::BackendKind::RemoteHttp;
        assert!(retrieve_memories(&request().with_judge_verdicts(inconsistent)).is_err());
    }

    #[test]
    fn judged_selection_is_deterministic_and_tracks_the_lexical_counterfactual() {
        let records = judgment_fixture_records();
        for policy in [
            AdmissionCombinationPolicy::BoundedAdditive,
            AdmissionCombinationPolicy::ReservedSlots,
        ] {
            let run = || {
                retrieve_memories(
                    &RetrievalRequest::new(
                        &records,
                        &[],
                        "orchid",
                        RetrievalStrategy::KeywordTag,
                        2,
                        evaluation_time(),
                    )
                    .with_judge_verdicts(judged_fixture_verdicts())
                    .with_combination_policy(policy),
                )
                .unwrap()
            };
            let first = run();
            let repeated = run();
            let lexical = retrieve_memories(&RetrievalRequest::new(
                &records,
                &[],
                "orchid",
                RetrievalStrategy::KeywordTag,
                2,
                evaluation_time(),
            ))
            .unwrap();

            assert_eq!(
                first.lexical_only_selected_ids,
                super::retrieved_memory_ids(&lexical.selected)
            );
            assert_eq!(
                first.lexical_only_selected_ids,
                ["memory.orchid-top", "memory.orchid-second"]
            );
            assert_eq!(first.selected, repeated.selected);
            assert_eq!(first.omitted, repeated.omitted);
            assert_eq!(
                first
                    .selected
                    .iter()
                    .find(|memory| memory.memory.id == "memory.orchid-top")
                    .unwrap()
                    .selection_eligibility,
                SelectionEligibility::Associable
            );
            assert_eq!(
                first
                    .selected
                    .iter()
                    .find(|memory| memory.memory.id == "memory.orchid-weak")
                    .unwrap()
                    .selection_eligibility,
                SelectionEligibility::JudgeInfluenced
            );
            assert!(first.selected.iter().any(|memory| {
                memory.memory.id == "memory.orchid-top"
                    && memory.admission_basis == AdmissionBasis::LexicalAndJudge
            }));
        }
    }

    fn verdicts(pairs: &[(&str, u16)]) -> BTreeMap<String, JudgeVerdict> {
        use qsf_semantics::trace::{BackendKind, ModelIdentity, ModelIdentityKind};

        pairs
            .iter()
            .map(|(memory_id, score_basis_points)| {
                (
                    (*memory_id).to_owned(),
                    JudgeVerdict {
                        score_basis_points: *score_basis_points,
                        model_identity: ModelIdentity {
                            backend: BackendKind::Fixture,
                            model_id: "fixture-memory-judge".to_owned(),
                            identity_kind: ModelIdentityKind::Fixture,
                            identity_value: "fixture-v1".to_owned(),
                        },
                        question_wording_version: "memory-relevance-v1".to_owned(),
                        backend_kind: BackendKind::Fixture,
                    },
                )
            })
            .collect()
    }

    fn judged_fixture_verdicts() -> BTreeMap<String, JudgeVerdict> {
        verdicts(&[
            ("memory.orchid-top", 4_000),
            ("memory.orchid-weak", 10_000),
            ("memory.zero-signal", 9_000),
        ])
    }

    fn judgment_fixture_records() -> Vec<MemoryRecord> {
        let mut top = test_record(
            "memory.orchid-top",
            "orchid archive",
            "A strong lexical orchid match.",
            vec!["orchid"],
            evaluation_time(),
        );
        let mut second = test_record(
            "memory.orchid-second",
            "orchid note",
            "A second lexical orchid match.",
            vec![],
            evaluation_time(),
        );
        let mut weak = test_record(
            "memory.orchid-weak",
            "orchid trace",
            "An old, weak lexical match.",
            vec![],
            evaluation_time() - time::Duration::days(3_650),
        );
        let mut zero_signal = test_record(
            "memory.zero-signal",
            "botanical practice",
            "Notes about tending plants.",
            vec![],
            evaluation_time(),
        );
        top.importance = 0.8;
        second.importance = 0.5;
        weak.importance = 0.0;
        zero_signal.importance = 0.8;
        vec![top, second, weak, zero_signal]
    }

    fn test_record(
        id: &str,
        title: &str,
        summary: &str,
        tags: Vec<&str>,
        created_at: OffsetDateTime,
    ) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Observation,
            title,
            summary,
            tags,
            created_at,
            0.8,
            0,
            "tests",
            10,
        )
    }

    fn evaluation_time() -> OffsetDateTime {
        OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap()
    }

    fn recency_ordering_records() -> Vec<MemoryRecord> {
        let mut older = test_record(
            "memory.older",
            "Older topic",
            "The topic is older.",
            vec![],
            evaluation_time() - time::Duration::days(31),
        );
        older.importance = 0.2;

        let mut newer = test_record(
            "memory.newer",
            "Newer topic",
            "The topic is newer.",
            vec![],
            evaluation_time(),
        );
        newer.importance = 0.0;

        vec![older, newer]
    }
}
