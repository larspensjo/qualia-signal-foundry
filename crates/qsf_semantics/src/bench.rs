//! Offline measurement harness for pair-scoring latency, request shaping, and cost.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use engine_logging::engine_warn;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    BlockingPairScorerService,
    backends::{
        fixture::{FixtureRelevanceJudge, FixtureRelevanceJudgeConfig},
        remote_http::RemoteRelevanceJudge,
    },
    budgets::MAX_ADDED_TIME_TO_FIRST_AUDIO_MS,
    config::{RelevanceJudgeBackendName, RelevanceJudgeConfig, SemanticConfigError},
    pair_scoring::{
        Candidate, CandidateKind, InjectionDeadline, PairScoreRequest, PairScoringOptions,
        PairScoringService, QuestionShaping, RelevanceTask, ScoreKind,
    },
    trace::{BackendKind, Traced},
};

/// Provider requests per minute documented by the hosted model provider.
pub const DOCUMENTED_REQUESTS_PER_MINUTE: u64 = 1_200;
/// Provider tokens per second documented for the hosted model.
pub const DOCUMENTED_TOKENS_PER_SECOND: u64 = 250_000;
/// Maximum total tokens documented for one hosted request.
pub const DOCUMENTED_MAX_TOKENS_PER_REQUEST: u64 = 64_000;
/// Maximum state plus longest-question tokens documented for one hosted request.
pub const DOCUMENTED_MAX_STATE_AND_QUESTION_TOKENS: u64 = 32_000;
/// Date the vendor model and rate-limit facts were read.
pub const PROVIDER_LIMITS_READ_DATE: &str = "2026-09-25";
/// Default sizes cover the live store and the 500-record target.
pub const DEFAULT_CANDIDATE_COUNTS_CSV: &str = "18,100,500";
/// Default repetitions keep retry reservations to a few hundred requests.
pub const DEFAULT_REPETITIONS: usize = 40;
/// Default measured turns for the small per-candidate cell.
pub const DEFAULT_PER_CANDIDATE_REPETITIONS: usize = 10;
/// Default maximum total request reservation.
pub const DEFAULT_MAX_TOTAL_REQUESTS: u64 = 400;
/// Default sustained-turn target used by the rate-limit plan.
pub const DEFAULT_TARGET_TURNS_PER_MINUTE: u64 = 10;
/// Memory and goal judge workloads share the provider request limit.
pub const DEFAULT_JUDGE_WORKLOADS_PER_TURN: u64 = 2;
/// Default turns used for conversation cost.
pub const DEFAULT_CONVERSATION_TURNS: u64 = 10;
/// A p99 is withheld until this many values are measured.
pub const MIN_SAMPLES_FOR_MEANINGFUL_P99: usize = 100;
/// Minimum successful end-to-end observations for a descriptive p95.
pub const MIN_SAMPLES_FOR_P95: usize = 20;
/// Stop a measured cell after this many failures without an intervening success.
pub const MAX_CONSECUTIVE_CELL_FAILURES: usize = 3;
const PREFLIGHT_ESTIMATOR: &str = "estimated input tokens are ceil(actual serialized hosted request bytes / 4); output is excluded; this is a planning heuristic, not vendor tokenization";
const PRICE_TABLE_TEXT: &str = include_str!("../prices/price-table.v1.json");

/// User-controlled settings. All candidate and utterance content is synthetic.
#[derive(Clone, Debug)]
pub struct BenchOptions {
    /// Candidate counts included in the measurement.
    pub candidate_counts: Vec<usize>,
    /// Repeated turns measured for each feasible cell.
    pub repetitions: usize,
    /// Repeated turns for the smallest per-candidate cell.
    pub per_candidate_repetitions: usize,
    /// Maximum worst-case HTTP attempts allowed across the run.
    pub max_total_requests: u64,
    /// Sustained turns per minute required for rate feasibility.
    pub target_turns_per_minute: u64,
    /// Number of judge workloads sharing provider RPM per spoken turn.
    pub judge_workloads_per_turn: u64,
    /// Turns assumed when deriving conversation cost.
    pub conversation_turns: u64,
    /// Operator-supplied non-judge local overhead in milliseconds.
    pub local_overhead_ms: u64,
    /// Operator-supplied network description.
    pub network_description: String,
    /// Directory receiving this run's artifacts.
    pub run_directory: PathBuf,
    /// Run id used to name artifacts.
    pub run_id: String,
    /// Persist and print a plan without calling the backend.
    pub dry_run: bool,
}

impl BenchOptions {
    /// Validates inputs that affect request planning and report arithmetic.
    pub fn validate(&self) -> Result<(), BenchError> {
        if self.candidate_counts.is_empty() || self.candidate_counts.contains(&0) {
            return Err(BenchError::InvalidOptions(
                "candidate counts must be a non-empty list of positive values".to_owned(),
            ));
        }
        if self
            .candidate_counts
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != self.candidate_counts.len()
        {
            return Err(BenchError::InvalidOptions(
                "candidate counts must be unique".to_owned(),
            ));
        }
        if self.repetitions == 0
            || self.per_candidate_repetitions == 0
            || self.target_turns_per_minute == 0
            || self.judge_workloads_per_turn == 0
        {
            return Err(BenchError::InvalidOptions(
                "repetitions and target turns per minute must be positive".to_owned(),
            ));
        }
        if self.conversation_turns == 0 || self.run_id.trim().is_empty() {
            return Err(BenchError::InvalidOptions(
                "conversation turns and run id must be present and positive".to_owned(),
            ));
        }
        if !self
            .run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(BenchError::InvalidOptions(
                "run id may contain only ASCII letters, digits, hyphens, and underscores"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

/// Backend settings captured from the existing explicit semantic configuration.
#[derive(Clone, Debug)]
pub struct BenchBackendSettings {
    /// Selected backend name.
    pub backend_name: String,
    /// Configured model id or fixture identity.
    pub pinned_model_id: String,
    /// Full request endpoint or fixture URI.
    pub endpoint: String,
    /// HTTP request timeout.
    pub request_timeout: Duration,
    /// Maximum attempts for each provider request.
    pub max_attempts: u32,
    /// Maximum simultaneous provider requests.
    pub max_concurrency: usize,
    /// Initial retry delay.
    pub initial_backoff: Duration,
    /// Maximum retry delay.
    pub max_backoff: Duration,
    /// Existing injection deadline supplied to the scoring service.
    pub configured_injection_deadline: Duration,
    /// Backend discriminator for the report.
    pub backend_kind: BackendKind,
}

impl BenchBackendSettings {
    /// Captures backend selection from the existing explicit config.
    pub fn from_config(config: &RelevanceJudgeConfig) -> Self {
        match config.backend {
            RelevanceJudgeBackendName::Fixture => Self {
                backend_name: "fixture".to_owned(),
                pinned_model_id: "fixture".to_owned(),
                endpoint: "fixture://local".to_owned(),
                request_timeout: config.request_timeout,
                max_attempts: 1,
                max_concurrency: 1,
                initial_backoff: Duration::ZERO,
                max_backoff: Duration::ZERO,
                configured_injection_deadline: config.injection_deadline,
                backend_kind: BackendKind::Fixture,
            },
            RelevanceJudgeBackendName::RemoteHttp => {
                let remote = config
                    .remote
                    .as_ref()
                    .expect("validated remote backend settings");
                Self {
                    backend_name: "remote_http".to_owned(),
                    pinned_model_id: remote.pinned_model_id.clone(),
                    endpoint: format!("{}/v1/systemone", remote.base_url.trim_end_matches('/')),
                    request_timeout: remote.request_timeout,
                    max_attempts: remote.max_attempts,
                    max_concurrency: remote.max_concurrency,
                    initial_backoff: remote.initial_backoff,
                    max_backoff: remote.max_backoff,
                    configured_injection_deadline: config.injection_deadline,
                    backend_kind: BackendKind::RemoteHttp,
                }
            }
        }
    }
}

/// Result of a measured run or request-only dry-run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BenchOutcome {
    /// Measurement report was written.
    Completed {
        /// Path to the generated structured report.
        report_path: PathBuf,
    },
    /// Request plan was written without calling the backend.
    DryRun {
        /// Path to the generated preflight plan.
        plan_path: PathBuf,
    },
}

/// Benchmark planning, execution, or artifact error.
#[derive(Debug, Error)]
pub enum BenchError {
    /// Measurement options are inconsistent.
    #[error("invalid bench options: {0}")]
    InvalidOptions(String),
    /// The existing backend configuration did not validate.
    #[error("relevance judge configuration failed: {0}")]
    Configuration(#[from] SemanticConfigError),
    /// Request cap refused the run before any calls were made.
    #[error(
        "planned worst-case request count {planned} exceeds maximum {maximum}; no requests were sent (plan: {plan_path})"
    )]
    RequestBudgetExceeded {
        /// Worst-case request count from the plan.
        planned: u64,
        /// Configured hard cap.
        maximum: u64,
        /// Persisted plan path.
        plan_path: PathBuf,
    },
    /// Selected backend could not be constructed.
    #[error("could not create selected backend: {0}")]
    Backend(String),
    /// Artifact filesystem operation failed.
    #[error("bench artifact operation failed at {path}: {source}")]
    Artifact {
        /// Path involved.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Backend exceeded the reserved request ceiling.
    #[error("bench cell {cell} used {actual} requests, beyond its reserved maximum {reserved}")]
    RequestBudgetInvariant {
        /// Cell identifier.
        cell: String,
        /// Observed attempts.
        actual: u64,
        /// Reserved attempts.
        reserved: u64,
    },
    /// JSON serialization or parsing failed.
    #[error("bench report serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// One shaping and candidate-count cell in the preflight plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchCellPlan {
    /// Requested question shaping.
    pub shaping: QuestionShaping,
    /// Number of synthetic candidates.
    pub candidate_count: usize,
    /// Number of measured turns when feasible.
    pub repetitions: usize,
    /// Measurement or derived-only treatment.
    pub measurement_status: String,
    /// Provider requests needed for one turn before retries.
    pub requests_per_turn: u64,
    /// Documented provider request-per-minute limit.
    pub requests_per_minute_limit: u64,
    /// Required sustained turns per minute.
    pub target_turns_per_minute: u64,
    /// Maximum sustainable spoken turns per minute with all judge workloads.
    pub maximum_sustainable_turns_per_minute_milli: u64,
    /// Result of provider request-rate arithmetic.
    pub rate_limit_feasible: bool,
    /// Arithmetic explanation if infeasible.
    pub infeasibility_basis: Option<String>,
    /// Worst-case physical requests reserved, including retries.
    pub planned_worst_case_requests: u64,
    /// Nominal physical requests without retries.
    pub planned_nominal_requests: u64,
    /// Token estimate for one turn; this is not provider tokenization.
    pub estimated_input_tokens_per_turn: u64,
    /// Largest heuristic request token estimate within this cell.
    pub estimated_max_input_tokens_per_request: u64,
    /// Largest heuristic state plus question token estimate.
    pub estimated_max_state_and_question_tokens: u64,
    /// Whether heuristic request size risks a documented cap.
    pub heuristic_token_limit_risk: bool,
    /// Estimated total cost across repetitions and retries, if priced.
    pub estimated_total_cost_nano_usd: Option<u64>,
}

/// Serialized request plan printed before execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchPlan {
    /// Plan schema version.
    pub schema_version: u32,
    /// Run identity.
    pub run_id: String,
    /// Selected backend.
    pub backend: String,
    /// Pinned model id.
    pub pinned_model_id: String,
    /// Endpoint that would receive requests.
    pub endpoint: String,
    /// Maximum requests accepted for the run.
    pub max_total_requests: u64,
    /// Total worst-case physical request reservation.
    pub planned_worst_case_total_requests: u64,
    /// Nominal sends, including one discarded warm-up request.
    pub planned_nominal_total_requests: u64,
    /// Estimated total cost when an exact model price exists.
    pub estimated_total_cost_nano_usd: Option<u64>,
    /// Method and limitations of the preflight token/cost estimate.
    pub estimate_method: String,
    /// Shaping and candidate-count cells.
    pub cells: Vec<BenchCellPlan>,
    /// Price table identity.
    pub price_table: PriceTableIdentity,
}

/// Provenance identity for the checked-in price table.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PriceTableIdentity {
    /// Checked-in price table version.
    pub version: String,
    /// Source of the price facts.
    pub source: String,
    /// Date the source was read.
    pub provenance_date: String,
    /// SHA-256 of the exact checked-in table bytes.
    pub content_sha256: String,
    /// Entry for the exact pinned model id, if priced.
    pub pinned_model_price: Option<ModelPrice>,
}

/// Checked-in input and output price for one exact model id.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPrice {
    /// Versioned model id.
    pub model_id: String,
    /// Input USD per million tokens.
    pub input_usd_per_million_tokens: String,
    /// Output USD per million tokens.
    pub output_usd_per_million_tokens: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PriceFile {
    version: String,
    provenance: PriceProvenance,
    models: std::collections::BTreeMap<String, PriceFileEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PriceProvenance {
    source: String,
    read_date: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PriceFileEntry {
    input_usd_per_million_tokens: String,
    output_usd_per_million_tokens: String,
}

/// Provider facts and operator-supplied non-judge inputs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurementInputs {
    /// Provider documentation source.
    pub provider_limits_source: String,
    /// Date provider limits were read.
    pub provider_limits_read_date: String,
    /// Provider request-per-minute limit.
    pub requests_per_minute: u64,
    /// Provider token-per-second limit.
    pub tokens_per_second: u64,
    /// Maximum total tokens in one request.
    pub max_tokens_per_request: u64,
    /// Maximum state plus longest-question tokens.
    pub max_state_and_question_tokens: u64,
    /// Operator-supplied non-judge local overhead.
    pub non_judge_local_overhead_ms: u64,
    /// Provenance statement for local overhead.
    pub non_judge_local_overhead_source: String,
    /// Rate target used to derive infeasibility.
    pub target_turns_per_minute: u64,
    /// Number of judge workloads per spoken turn.
    pub judge_workloads_per_turn: u64,
    /// Bench backend concurrency and retry policy.
    pub max_concurrency: usize,
    /// Maximum attempts per request.
    pub max_attempts: u32,
    /// Initial retry delay in milliseconds.
    pub initial_backoff_ms: u64,
    /// Maximum retry delay in milliseconds.
    pub max_backoff_ms: u64,
}

/// Nearest-rank percentile with its sample count.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PercentileEstimate {
    /// Percentile rank.
    pub percentile: u8,
    /// Number of values included.
    pub sample_count: usize,
    /// Microsecond value; withheld for undersampled p99.
    pub value_micros: Option<u64>,
    /// Estimate status.
    pub status: String,
}

/// p50, p95, and p99 latency estimates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PercentileSet {
    /// Median estimate and count.
    pub p50: PercentileEstimate,
    /// 95th percentile estimate and count.
    pub p95: PercentileEstimate,
    /// 99th percentile estimate and count.
    pub p99: PercentileEstimate,
}

/// Token usage totals and measured cost from provider-reported usage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UsageSummary {
    /// Scored turns with provider-reported input and output usage.
    pub observed_turns: u64,
    /// Successful provider responses represented by usage.
    pub observed_requests: u64,
    /// Provider-reported input token total.
    pub total_input_tokens: u64,
    /// Provider-reported output token total.
    pub total_output_tokens: u64,
    /// Rounded mean input tokens per provider response.
    pub mean_input_tokens_per_request: Option<u64>,
    /// Rounded mean output tokens per provider response.
    pub mean_output_tokens_per_request: Option<u64>,
    /// Measured cost if the exact model has a local price entry.
    pub cost: Option<MeasuredCost>,
    /// Whether price was measured, unavailable, or not listed.
    pub cost_status: String,
}

/// Usage-derived cost per turn and for one stated conversation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredCost {
    /// Total measured cost in nano-USD.
    pub total_nano_usd: u64,
    /// Number of usage-bearing turns.
    pub observed_turns: u64,
    /// Mean measured cost per turn, rounded to nearest nano-USD.
    pub cost_per_turn_nano_usd: u64,
    /// Assumed turns in one conversation.
    pub conversation_turns: u64,
    /// Derived cost per conversation in nano-USD.
    pub cost_per_conversation_nano_usd: u64,
}

/// One backend result without storing synthetic input text in the artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchSample {
    /// Repetition index within the cell.
    pub repetition: usize,
    /// Backend invocation id.
    pub invocation_id: String,
    /// Model ids resolved by completed backend requests.
    pub resolved_model_ids: Vec<String>,
    /// End-to-end latency in microseconds.
    pub end_to_end_latency_micros: u64,
    /// Latency per physical HTTP attempt.
    pub attempt_latency_micros: Vec<u64>,
    /// Total HTTP attempts made.
    pub attempt_count: u32,
    /// Successful provider responses with parsed usage.
    pub successful_provider_responses: u64,
    /// Largest provider-reported input token count among successful responses.
    pub max_input_tokens_per_request: Option<u64>,
    /// Retry reasons returned by the service.
    pub retry_reasons: Vec<String>,
    /// Provider-reported input tokens.
    pub input_tokens: Option<u64>,
    /// Provider-reported output tokens.
    pub output_tokens: Option<u64>,
    /// Typed service failure, if any.
    pub failure: Option<String>,
}

/// Structured result for one shaping and candidate-count combination.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchCellReport {
    /// Original preflight plan.
    pub plan: BenchCellPlan,
    /// Number of end-to-end samples.
    pub sample_count: usize,
    /// Failed invocations excluded from end-to-end percentiles.
    pub failed_invocations: u64,
    /// Failure counts by typed failure string.
    pub failure_histogram: BTreeMap<String, u64>,
    /// Early stop after consecutive failed invocations, if one occurred.
    #[serde(default)]
    pub stop: Option<BenchCellStop>,
    /// Derived end-to-end estimate for unmeasured per-candidate shapes.
    pub derived_end_to_end_p95_micros: Option<u64>,
    /// End-to-end latency percentiles.
    pub end_to_end_latency: PercentileSet,
    /// Physical per-attempt latency percentiles.
    pub per_attempt_latency: PercentileSet,
    /// Number of invocations with one or more retries.
    pub retried_invocation_count: u64,
    /// Number of invocations used for retry incidence.
    pub retry_incidence_sample_count: u64,
    /// Retry-incidence numerator and denominator.
    pub retry_incidence: RetryIncidence,
    /// Sum of backend attempts recorded in traces.
    pub observed_attempt_count: u64,
    /// Token and cost metrics.
    pub usage: UsageSummary,
    /// Whether measured p95 plus local overhead met the fixed limit.
    pub p95_fits_added_time_limit: Option<bool>,
    /// Projected added p95 with stated local overhead.
    pub projected_added_time_p95_micros: Option<u64>,
    /// Whole-cell wall time.
    pub cell_wall_time_micros: u64,
    /// Per-repetition observations.
    pub samples: Vec<BenchSample>,
}

/// Why a measured cell stopped before its planned repetitions completed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchCellStop {
    /// Machine-readable stop reason.
    pub status: String,
    /// Failed invocations in the final consecutive streak.
    pub consecutive_failures: usize,
    /// Failure returned by the final invocation.
    pub last_failure_reason: String,
}

/// Retry incidence numerator and denominator.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetryIncidence {
    /// Invocations with at least one retried request.
    pub retried_invocations: u64,
    /// Total invocations.
    pub invocations: u64,
}

/// Injection deadline derived under the fixed added-time budget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedInjectionDeadline {
    /// Chosen injection wait, never above the fixed limit.
    pub deadline_ms: Option<u64>,
    /// Maximum deadline permitted after stated local overhead.
    pub maximum_permitted_ms: u64,
    /// Sample count behind the p95 that selected this value.
    pub source_sample_count: Option<usize>,
    /// Whether any measured, rate-feasible cell fit the fixed limit.
    pub any_measured_shaping_fits: bool,
    /// Whether a shaping at the largest candidate count fit.
    pub any_shape_fits_largest_store: bool,
    /// Finding recorded when no measured shaping fits.
    pub finding: Option<String>,
    /// Per-shaping result at the largest configured store.
    pub per_shaping: Vec<ShapingDeadline>,
    /// Largest measured fitting store for each shaping, when one exists.
    #[serde(default)]
    pub largest_fitting_store_by_shaping: Vec<FittingStoreDeadline>,
    /// Largest fitting store across shapings; ties use the lower deadline.
    #[serde(default)]
    pub largest_fitting_store: Option<FittingStoreDeadline>,
}

/// Measured store with enough failure-free samples to support a deadline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FittingStoreDeadline {
    /// Question-shaping policy.
    pub shaping: QuestionShaping,
    /// Number of candidates in the measured store.
    pub candidate_count: usize,
    /// p95-derived deadline capped at the maximum permitted wait.
    pub deadline_ms: u64,
    /// Successful observations behind the p95.
    pub sample_count: usize,
}

/// Deadline assessment for one shaping at the largest configured store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShapingDeadline {
    /// Question-shaping policy.
    pub shaping: QuestionShaping,
    /// Largest configured candidate count.
    pub candidate_count: usize,
    /// Candidate deadline when supported by sufficient successful samples.
    pub deadline_ms: Option<u64>,
    /// Whether this shaping fits the fixed limit.
    pub fits: bool,
    /// Source status or exclusion reason.
    pub status: String,
}

/// Complete structured report from one benchmark run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BenchReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Run identity.
    pub run_id: String,
    /// UTC execution start time.
    pub date_utc: String,
    /// Backend discriminator.
    pub backend: BackendKind,
    /// Pinned model requested from the service.
    pub pinned_model_id: String,
    /// Resolved model ids returned by the service.
    pub resolved_model_ids: Vec<String>,
    /// Exact POST endpoint used.
    pub endpoint: String,
    /// Versioned question wording.
    pub question_wording_version: String,
    /// Machine and network provenance.
    pub environment: MachineEnvironment,
    /// Provider limits and local overhead inputs.
    pub inputs: MeasurementInputs,
    /// Local pricing provenance.
    pub price_table: PriceTableIdentity,
    /// Method and limitations of the request-plan estimate.
    pub preflight_estimate_method: String,
    /// Request timeout, distinct from injection deadline.
    pub request_timeout_ms: u64,
    /// Configured injection deadline passed to backend traces.
    pub configured_injection_deadline_ms: u64,
    /// Actual physical requests issued during this run.
    pub actual_requests_sent: u64,
    /// Whether the request cap stopped measurements.
    pub stopped_at_request_cap: bool,
    /// Derived injection deadline and 300 ms fit.
    pub derived_injection_deadline: DerivedInjectionDeadline,
    /// Turns assumed per conversation.
    pub conversation_turns_assumption: u64,
    /// Per-cell measurements and derived infeasibility.
    pub cells: Vec<BenchCellReport>,
    /// Explicit derived findings.
    pub findings: Vec<String>,
}

/// Machine and network description captured for one run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MachineEnvironment {
    /// Computer name from the environment, if available.
    pub machine_name: String,
    /// Operating-system identifier.
    pub operating_system: String,
    /// CPU architecture identifier.
    pub architecture: String,
    /// Available logical CPU count.
    pub logical_cpu_count: Option<usize>,
    /// Operator-supplied network description.
    pub network_description: String,
    /// Source commit read from the local Git checkout.
    pub code_commit: String,
    /// Whether tracked or untracked code differs from the commit.
    pub code_tree_dirty: Option<bool>,
}

/// Loads the price table and hashes its exact checked-in bytes.
pub fn price_table_identity(model_id: &str) -> Result<PriceTableIdentity, BenchError> {
    let parsed: PriceFile = serde_json::from_str(PRICE_TABLE_TEXT)?;
    let pinned_model_price = parsed.models.get(model_id).map(|price| ModelPrice {
        model_id: model_id.to_owned(),
        input_usd_per_million_tokens: price.input_usd_per_million_tokens.clone(),
        output_usd_per_million_tokens: price.output_usd_per_million_tokens.clone(),
    });
    Ok(PriceTableIdentity {
        version: parsed.version,
        source: parsed.provenance.source,
        provenance_date: parsed.provenance.read_date,
        content_sha256: hex(&Sha256::digest(PRICE_TABLE_TEXT.as_bytes())),
        pinned_model_price,
    })
}

/// Computes worst-case request reservations and preflight cost estimates.
pub fn plan_run(
    options: &BenchOptions,
    backend: &BenchBackendSettings,
) -> Result<BenchPlan, BenchError> {
    options.validate()?;
    if backend.backend_kind == BackendKind::RemoteHttp && backend.pinned_model_id == "jev-latest" {
        return Err(BenchError::InvalidOptions(
            "remote bench requires a pinned versioned model id; jev-latest is a moving alias"
                .to_owned(),
        ));
    }
    let price_table = price_table_identity(&backend.pinned_model_id)?;
    let price = price_table.pinned_model_price.as_ref();
    let max_attempts = u64::from(backend.max_attempts.max(1));
    let mut cells = Vec::new();
    for shaping in [
        QuestionShaping::SharedStateQuestions,
        QuestionShaping::PerCandidateRequest,
    ] {
        let mut counts = options.candidate_counts.clone();
        counts.sort_unstable();
        for candidate_count in &counts {
            let requests_per_turn = match shaping {
                QuestionShaping::SharedStateQuestions => 1,
                QuestionShaping::PerCandidateRequest => *candidate_count as u64,
            };
            let maximum_sustainable_turns_per_minute_milli = DOCUMENTED_REQUESTS_PER_MINUTE
                .saturating_mul(1_000)
                / requests_per_turn.saturating_mul(options.judge_workloads_per_turn);
            let rate_limit_feasible = DOCUMENTED_REQUESTS_PER_MINUTE
                >= requests_per_turn
                    .saturating_mul(options.target_turns_per_minute)
                    .saturating_mul(options.judge_workloads_per_turn);
            let infeasibility_basis = (!rate_limit_feasible).then(|| {
                format!(
                    "derived by rate-limit arithmetic: {requests_per_turn} request(s)/workload × {} workloads/turn × {} turns/minute exceeds {DOCUMENTED_REQUESTS_PER_MINUTE} requests/minute; maximum is {:.2} turns/minute",
                    options.judge_workloads_per_turn,
                    options.target_turns_per_minute,
                    maximum_sustainable_turns_per_minute_milli as f64 / 1_000.0
                )
            });
            let measured = rate_limit_feasible
                && (shaping == QuestionShaping::SharedStateQuestions
                    || *candidate_count == *counts.first().expect("nonempty"));
            let repetitions = if measured {
                if shaping == QuestionShaping::SharedStateQuestions {
                    options.repetitions
                } else {
                    options.per_candidate_repetitions
                }
            } else {
                0
            };
            let planned_nominal_requests = if measured {
                requests_per_turn.saturating_mul(repetitions as u64)
            } else {
                0
            };
            let planned_worst_case_requests = if measured {
                requests_per_turn
                    .saturating_mul(repetitions as u64)
                    .saturating_mul(max_attempts)
            } else {
                0
            };
            let request = synthetic_request(&synthetic_candidates(*candidate_count), 0, shaping);
            let sizes = crate::backends::remote_http::request_body_sizes(
                &request,
                &backend.pinned_model_id,
            );
            let estimated_input_tokens_per_turn: u64 = sizes
                .iter()
                .map(|(bytes, _)| (*bytes as u64).div_ceil(4))
                .sum();
            let estimated_max_input_tokens_per_request = sizes
                .iter()
                .map(|(bytes, _)| (*bytes as u64).div_ceil(4))
                .max()
                .unwrap_or_default();
            let estimated_max_state_and_question_tokens = sizes
                .iter()
                .map(|(_, bytes)| (*bytes as u64).div_ceil(4))
                .max()
                .unwrap_or_default();
            let heuristic_token_limit_risk = estimated_max_input_tokens_per_request
                >= DOCUMENTED_MAX_TOKENS_PER_REQUEST
                || estimated_max_state_and_question_tokens
                    >= DOCUMENTED_MAX_STATE_AND_QUESTION_TOKENS;
            let estimated_total_cost_nano_usd = if measured {
                price.and_then(|price| {
                    price_cost_nano_usd(
                        estimated_input_tokens_per_turn
                            .saturating_mul(repetitions as u64)
                            .saturating_mul(max_attempts),
                        0,
                        price,
                    )
                })
            } else {
                None
            };
            cells.push(BenchCellPlan {
                shaping,
                candidate_count: *candidate_count,
                repetitions,
                measurement_status: if measured {
                    "measured"
                } else if shaping == QuestionShaping::PerCandidateRequest {
                    "derived_not_measured"
                } else {
                    "not_measured"
                }
                .to_owned(),
                requests_per_turn,
                requests_per_minute_limit: DOCUMENTED_REQUESTS_PER_MINUTE,
                target_turns_per_minute: options.target_turns_per_minute,
                maximum_sustainable_turns_per_minute_milli,
                rate_limit_feasible,
                infeasibility_basis,
                planned_worst_case_requests,
                planned_nominal_requests,
                estimated_input_tokens_per_turn,
                estimated_max_input_tokens_per_request,
                estimated_max_state_and_question_tokens,
                heuristic_token_limit_risk,
                estimated_total_cost_nano_usd,
            });
        }
    }
    let planned_total: u64 = cells
        .iter()
        .map(|cell| cell.planned_worst_case_requests)
        .sum::<u64>()
        .saturating_add(max_attempts);
    let planned_nominal_total_requests = cells
        .iter()
        .map(|cell| cell.planned_nominal_requests)
        .sum::<u64>()
        .saturating_add(1);
    let estimated_total = price.map(|price| {
        let cell_cost = cells
            .iter()
            .filter(|cell| cell.rate_limit_feasible)
            .filter_map(|cell| cell.estimated_total_cost_nano_usd)
            .fold(0_u64, u64::saturating_add);
        let warmup_request = synthetic_request(
            &synthetic_candidates(1),
            0,
            QuestionShaping::SharedStateQuestions,
        );
        let warmup_tokens = crate::backends::remote_http::request_body_sizes(
            &warmup_request,
            &backend.pinned_model_id,
        )[0]
        .0
        .div_ceil(4) as u64;
        cell_cost.saturating_add(
            price_cost_nano_usd(warmup_tokens.saturating_mul(max_attempts), 0, price)
                .unwrap_or_default(),
        )
    });
    Ok(BenchPlan {
        schema_version: 1,
        run_id: options.run_id.clone(),
        backend: backend.backend_name.clone(),
        pinned_model_id: backend.pinned_model_id.clone(),
        endpoint: backend.endpoint.clone(),
        max_total_requests: options.max_total_requests,
        planned_worst_case_total_requests: planned_total,
        planned_nominal_total_requests,
        estimated_total_cost_nano_usd: estimated_total,
        estimate_method: PREFLIGHT_ESTIMATOR.to_owned(),
        cells,
        price_table,
    })
}

/// Pure human-readable rendering of a preflight plan.
pub fn render_plan(plan: &BenchPlan) -> String {
    let mut output = format!(
        "Bench plan: run={} backend={} model={} endpoint={}\n",
        plan.run_id, plan.backend, plan.pinned_model_id, plan.endpoint
    );
    for cell in &plan.cells {
        let status = if cell.measurement_status == "measured" {
            format!(
                "nominal={} worst-case={} HTTP requests",
                cell.planned_nominal_requests, cell.planned_worst_case_requests
            )
        } else if cell.rate_limit_feasible {
            "derived-not-measured (per-attempt latency × concurrency waves)".to_owned()
        } else {
            format!(
                "infeasible (derived): {}",
                cell.infeasibility_basis.as_deref().unwrap_or("rate limit")
            )
        };
        let cost = cell
            .estimated_total_cost_nano_usd
            .map(format_usd_nano)
            .unwrap_or_else(|| "tokens only / no price estimate".to_owned());
        output.push_str(&format!(
            "  shaping={} candidates={} repetitions={} requests/turn={} max_sustained={:.2} turns/min target={}/min token_limit_risk={} {} estimated_cost={}\n",
            shaping_name(cell.shaping),
            cell.candidate_count,
            cell.repetitions,
            cell.requests_per_turn,
            cell.maximum_sustainable_turns_per_minute_milli as f64 / 1_000.0,
            cell.target_turns_per_minute,
            cell.heuristic_token_limit_risk,
            status,
            cost
        ));
    }
    let total_cost = plan
        .estimated_total_cost_nano_usd
        .map(format_usd_nano)
        .unwrap_or_else(|| "tokens only / no price estimate".to_owned());
    output.push_str(&format!(
        "Total nominal planned requests: {} / {}\nTotal worst-case planned requests: {} / {}\nEstimated total cost: {}\n",
        plan.planned_nominal_total_requests, plan.max_total_requests, plan.planned_worst_case_total_requests, plan.max_total_requests, total_cost
    ));
    output.push_str(&format!("Estimate method: {}\n", plan.estimate_method));
    output.push_str(&format!(
        "A measured cell stops after {MAX_CONSECUTIVE_CELL_FAILURES} consecutive failed invocations.\n"
    ));
    output
}

/// Pure human-readable rendering of a completed report.
pub fn render_report_summary(report: &BenchReport) -> String {
    let mut output = format!(
        "Bench report: run={} backend={:?} model={} endpoint={} date={}\n",
        report.run_id, report.backend, report.pinned_model_id, report.endpoint, report.date_utc
    );
    for cell in &report.cells {
        if !cell.plan.rate_limit_feasible {
            output.push_str(&format!(
                "  shaping={} candidates={} status={} derived_end_to_end_p95_micros={:?} infeasible (derived): {}\n",
                shaping_name(cell.plan.shaping),
                cell.plan.candidate_count,
                cell.plan.measurement_status,
                cell.derived_end_to_end_p95_micros,
                cell.plan
                    .infeasibility_basis
                    .as_deref()
                    .unwrap_or("rate limit")
            ));
            continue;
        }
        output.push_str(&format!(
            "  shaping={} candidates={} status={} stop={:?} requests/turn={} actual_attempts={} failures={}/{} retry_incidence={}/{} end_to_end[{}] per_attempt[{}] derived_end_to_end_p95_micros={:?} input_tokens/request={} output_tokens/request={} ",
            shaping_name(cell.plan.shaping),
            cell.plan.candidate_count,
            cell.plan.measurement_status,
            cell.stop,
            cell.plan.requests_per_turn,
            cell.observed_attempt_count,
            cell.failed_invocations,
            cell.sample_count,
            cell.retry_incidence.retried_invocations,
            cell.retry_incidence.invocations,
            render_percentiles(&cell.end_to_end_latency),
            render_percentiles(&cell.per_attempt_latency),
            cell.derived_end_to_end_p95_micros,
            cell.usage.mean_input_tokens_per_request.map_or_else(|| "n/a".to_owned(), |value| value.to_string()),
            cell.usage.mean_output_tokens_per_request.map_or_else(|| "n/a".to_owned(), |value| value.to_string()),
        ));
        match &cell.usage.cost {
            Some(cost) => output.push_str(&format!(
                "cost/turn={} cost/conversation={} ({} turns)\n",
                format_usd_nano(cost.cost_per_turn_nano_usd),
                format_usd_nano(cost.cost_per_conversation_nano_usd),
                cost.conversation_turns
            )),
            None => output.push_str(&format!(
                "cost={} tokens_only={}\n",
                cell.usage.cost_status,
                cell.usage
                    .total_input_tokens
                    .saturating_add(cell.usage.total_output_tokens)
            )),
        }
    }
    match report.derived_injection_deadline.deadline_ms {
        Some(deadline) => output.push_str(&format!(
            "Derived injection deadline: {deadline} ms (maximum permitted {} ms); request timeout: {} ms\n",
            report.derived_injection_deadline.maximum_permitted_ms,
            report.request_timeout_ms
        )),
        None => output.push_str(&format!(
            "Derived injection deadline: none; request timeout: {} ms\n",
            report.request_timeout_ms
        )),
    }
    for shaping in &report.derived_injection_deadline.per_shaping {
        output.push_str(&format!(
            "  largest-store shaping={} candidates={} deadline_ms={:?} fits={} status={}\n",
            shaping_name(shaping.shaping),
            shaping.candidate_count,
            shaping.deadline_ms,
            shaping.fits,
            shaping.status
        ));
    }
    match &report.derived_injection_deadline.largest_fitting_store {
        Some(store) => output.push_str(&format!(
            "Largest store that fits: {} candidates, shaping={}, deadline {} ms (p95 over {} samples)\n",
            store.candidate_count,
            shaping_name(store.shaping),
            store.deadline_ms,
            store.sample_count
        )),
        None => output.push_str("Largest store that fits: none\n"),
    }
    for finding in &report.findings {
        output.push_str(&format!("Finding: {finding}\n"));
    }
    output
}

/// Loads the existing explicit config and runs the backend it names.
pub async fn run_from_env(options: BenchOptions) -> Result<BenchOutcome, BenchError> {
    let config = RelevanceJudgeConfig::from_env()?;
    let backend = BenchBackendSettings::from_config(&config);
    match config.backend {
        RelevanceJudgeBackendName::Fixture => {
            let service = BlockingPairScorerService::new(FixtureRelevanceJudge::new(
                FixtureRelevanceJudgeConfig::default(),
            ));
            run_with_service(options, backend, &service).await
        }
        RelevanceJudgeBackendName::RemoteHttp => {
            let remote = config
                .remote
                .clone()
                .expect("validated remote backend configuration");
            let service = RemoteRelevanceJudge::new(remote)
                .map_err(|error| BenchError::Backend(error.to_string()))?;
            let (service, sent) = service.with_request_budget(options.max_total_requests);
            run_with_service_counted(options, backend, &service, Some(sent)).await
        }
    }
}

/// Persists the plan, enforces its request cap, measures, and writes a report.
pub async fn run_with_service(
    options: BenchOptions,
    backend: BenchBackendSettings,
    service: &dyn PairScoringService,
) -> Result<BenchOutcome, BenchError> {
    run_with_service_counted(options, backend, service, None).await
}

async fn run_with_service_counted(
    options: BenchOptions,
    backend: BenchBackendSettings,
    service: &dyn PairScoringService,
    physical_sent: Option<Arc<AtomicU64>>,
) -> Result<BenchOutcome, BenchError> {
    let plan = plan_run(&options, &backend)?;
    print!("{}", render_plan(&plan));
    fs::create_dir_all(&options.run_directory).map_err(|source| BenchError::Artifact {
        path: options.run_directory.clone(),
        source,
    })?;
    let plan_path = options.run_directory.join("bench-plan.json");
    write_json(&plan_path, &plan)?;
    if plan.planned_nominal_total_requests > options.max_total_requests {
        return Err(BenchError::RequestBudgetExceeded {
            planned: plan.planned_nominal_total_requests,
            maximum: options.max_total_requests,
            plan_path,
        });
    }
    if options.dry_run {
        return Ok(BenchOutcome::DryRun { plan_path });
    }

    let start_date = now_rfc3339();
    let price = plan.price_table.pinned_model_price.as_ref();
    let mut reports = Vec::new();
    let mut resolved_ids = BTreeSet::new();
    let mut traced_attempts = 0_u64;
    let mut stopped_at_request_cap = false;
    // Discard the first call so handshake setup does not enter any percentile.
    let warmup_candidates = synthetic_candidates(1);
    let warmup = service
        .score_pairs(
            synthetic_request(&warmup_candidates, 0, QuestionShaping::SharedStateQuestions),
            InjectionDeadline::from_duration(backend.configured_injection_deadline),
        )
        .await;
    traced_attempts = traced_attempts.saturating_add(u64::from(
        warmup
            .trace
            .service
            .as_ref()
            .map_or(0, |payload| payload.attempt_count),
    ));
    for cell_plan in &plan.cells {
        if cell_plan.measurement_status != "measured" || stopped_at_request_cap {
            reports.push(aggregate_cell(
                cell_plan.clone(),
                Vec::new(),
                Duration::ZERO,
                options.local_overhead_ms,
                options.conversation_turns,
                price,
            ));
            continue;
        }
        let cell_started = Instant::now();
        let candidates = synthetic_candidates(cell_plan.candidate_count);
        let mut samples = Vec::new();
        let mut consecutive_failures = 0;
        let mut cell_stop = None;
        for repetition in 0..cell_plan.repetitions {
            let sent = physical_sent
                .as_ref()
                .map_or(traced_attempts, |value| value.load(Ordering::SeqCst));
            if sent.saturating_add(cell_plan.requests_per_turn) > options.max_total_requests {
                stopped_at_request_cap = true;
                break;
            }
            let request = synthetic_request(&candidates, repetition, cell_plan.shaping);
            let traced = service
                .score_pairs(
                    request,
                    InjectionDeadline::from_duration(backend.configured_injection_deadline),
                )
                .await;
            if let Some(failure) = &traced.trace.failure {
                engine_warn!(
                    "semantic bench invocation failed invocation_id={} endpoint={} cell={}x{} repetition={} failure={}",
                    traced.trace.invocation_id,
                    backend.endpoint,
                    shaping_name(cell_plan.shaping),
                    cell_plan.candidate_count,
                    repetition,
                    failure
                );
            }
            let sample = sample_from_trace(repetition, &traced);
            if let Some(failure) = &sample.failure {
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_CELL_FAILURES {
                    cell_stop = Some(BenchCellStop {
                        status: "stopped_after_consecutive_failures".to_owned(),
                        consecutive_failures,
                        last_failure_reason: failure.clone(),
                    });
                    engine_warn!(
                        "semantic bench cell stopped early endpoint={} cell={}x{} after {} consecutive failed invocations last_failure={}",
                        backend.endpoint,
                        shaping_name(cell_plan.shaping),
                        cell_plan.candidate_count,
                        consecutive_failures,
                        failure
                    );
                }
            } else {
                consecutive_failures = 0;
            }
            traced_attempts = traced_attempts.saturating_add(u64::from(sample.attempt_count));
            if sample
                .failure
                .as_deref()
                .is_some_and(|failure| failure.contains("bench_request_cap_reached"))
            {
                stopped_at_request_cap = true;
            }
            if sample.failure.is_some()
                && physical_sent.as_ref().is_some_and(|sent| {
                    sent.load(Ordering::SeqCst) >= options.max_total_requests
                        && u64::from(sample.attempt_count) < cell_plan.requests_per_turn
                })
            {
                stopped_at_request_cap = true;
            }
            resolved_ids.extend(sample.resolved_model_ids.iter().cloned());
            let reserved = cell_plan
                .requests_per_turn
                .saturating_mul(u64::from(backend.max_attempts.max(1)));
            if u64::from(sample.attempt_count) > reserved {
                return Err(BenchError::RequestBudgetInvariant {
                    cell: format!(
                        "{}x{}",
                        shaping_name(cell_plan.shaping),
                        cell_plan.candidate_count
                    ),
                    actual: u64::from(sample.attempt_count),
                    reserved,
                });
            }
            samples.push(sample);
            if stopped_at_request_cap || cell_stop.is_some() {
                break;
            }
        }
        let mut report = aggregate_cell(
            cell_plan.clone(),
            samples,
            cell_started.elapsed(),
            options.local_overhead_ms,
            options.conversation_turns,
            price,
        );
        report.stop = cell_stop;
        reports.push(report);
    }
    let reports = with_derived_per_candidate_latency(reports, backend.max_concurrency);
    let largest_count = *options.candidate_counts.iter().max().unwrap_or(&0);
    let deadline = derive_injection_deadline(
        &reports,
        largest_count,
        options.local_overhead_ms,
        backend.request_timeout.as_millis() as u64,
    );
    let mut findings = build_findings(&reports, &deadline, &backend, largest_count);
    if stopped_at_request_cap {
        findings.push(format!(
            "run stopped cleanly at the {} physical-request cap; remaining cells were not measured",
            options.max_total_requests
        ));
    }
    if options.local_overhead_ms == 0 {
        findings.push(
            "local overhead is 0 ms; latency fit is a lower bound, not application-path evidence"
                .to_owned(),
        );
    }
    for cell in &reports {
        if cell.plan.heuristic_token_limit_risk {
            findings.push(format!(
                "heuristic token-limit risk for {} at {} candidates",
                shaping_name(cell.plan.shaping),
                cell.plan.candidate_count
            ));
        }
        if cell.failed_invocations > 0 {
            findings.push(format!("{} at {} candidates had {} failed invocation(s); excluded from deadline derivation", shaping_name(cell.plan.shaping), cell.plan.candidate_count, cell.failed_invocations));
        }
        if let Some(tokens) = cell
            .samples
            .iter()
            .filter_map(|sample| sample.max_input_tokens_per_request)
            .max()
        {
            if tokens >= DOCUMENTED_MAX_TOKENS_PER_REQUEST {
                findings.push(format!("observed per-request input tokens meet or exceed documented {} total-token cap for {} at {} candidates", DOCUMENTED_MAX_TOKENS_PER_REQUEST, shaping_name(cell.plan.shaping), cell.plan.candidate_count));
            }
            if tokens >= DOCUMENTED_MAX_STATE_AND_QUESTION_TOKENS {
                findings.push(format!("observed per-request input tokens are at least the documented {} state-plus-question cap for {} at {} candidates; provider usage does not separate state and question tokens", DOCUMENTED_MAX_STATE_AND_QUESTION_TOKENS, shaping_name(cell.plan.shaping), cell.plan.candidate_count));
            }
        }
    }
    let report = BenchReport {
        schema_version: 1,
        run_id: options.run_id.clone(),
        date_utc: start_date,
        backend: backend.backend_kind,
        pinned_model_id: backend.pinned_model_id.clone(),
        resolved_model_ids: resolved_ids.into_iter().collect(),
        endpoint: backend.endpoint.clone(),
        question_wording_version: crate::backends::remote_http::QUESTION_WORDING_VERSION
            .to_owned(),
        environment: machine_environment(&options.network_description),
        inputs: MeasurementInputs {
            provider_limits_source: "https://docs.typesafe.ai/models".to_owned(),
            provider_limits_read_date: PROVIDER_LIMITS_READ_DATE.to_owned(),
            requests_per_minute: DOCUMENTED_REQUESTS_PER_MINUTE,
            tokens_per_second: DOCUMENTED_TOKENS_PER_SECOND,
            max_tokens_per_request: DOCUMENTED_MAX_TOKENS_PER_REQUEST,
            max_state_and_question_tokens: DOCUMENTED_MAX_STATE_AND_QUESTION_TOKENS,
            non_judge_local_overhead_ms: options.local_overhead_ms,
            non_judge_local_overhead_source: "operator-supplied --local-overhead-ms for candidate assembly + context assembly + send; default 0 ms is a lower-bound assumption, not an application-path measurement".to_owned(),
            target_turns_per_minute: options.target_turns_per_minute,
            judge_workloads_per_turn: options.judge_workloads_per_turn,
            max_concurrency: backend.max_concurrency,
            max_attempts: backend.max_attempts,
            initial_backoff_ms: backend.initial_backoff.as_millis() as u64,
            max_backoff_ms: backend.max_backoff.as_millis() as u64,
        },
        price_table: plan.price_table,
        preflight_estimate_method: plan.estimate_method,
        request_timeout_ms: backend.request_timeout.as_millis() as u64,
        configured_injection_deadline_ms: backend.configured_injection_deadline.as_millis() as u64,
        actual_requests_sent: physical_sent.as_ref().map_or(traced_attempts, |value| value.load(Ordering::SeqCst)),
        stopped_at_request_cap,
        derived_injection_deadline: deadline,
        conversation_turns_assumption: options.conversation_turns,
        cells: reports,
        findings,
    };
    let report_path = options.run_directory.join("bench-report.json");
    write_json(&report_path, &report)?;
    print!("{}", render_report_summary(&report));
    Ok(BenchOutcome::Completed { report_path })
}

/// Calculates a nearest-rank percentile from an integer latency slice.
pub fn nearest_rank_percentile(values: &[u64], percentile: u8) -> Option<u64> {
    if values.is_empty() || percentile == 0 || percentile > 100 {
        return None;
    }
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let rank = (ordered.len() * usize::from(percentile)).div_ceil(100);
    ordered.get(rank.saturating_sub(1)).copied()
}

/// Derives exact model cost from provider-reported usage and the local price table.
pub fn price_cost_nano_usd(
    input_tokens: u64,
    output_tokens: u64,
    price: &ModelPrice,
) -> Option<u64> {
    let input_pico = (input_tokens as u128)
        .checked_mul(pico_usd_per_token(&price.input_usd_per_million_tokens)?)?;
    let output_pico = (output_tokens as u128)
        .checked_mul(pico_usd_per_token(&price.output_usd_per_million_tokens)?)?;
    u64::try_from(input_pico.checked_add(output_pico)?.checked_add(500)? / 1_000).ok()
}

/// Derives an injection deadline under the fixed added-time budget.
pub fn derive_injection_deadline(
    cells: &[BenchCellReport],
    largest_candidate_count: usize,
    local_overhead_ms: u64,
    request_timeout_ms: u64,
) -> DerivedInjectionDeadline {
    let limit_micros = MAX_ADDED_TIME_TO_FIRST_AUDIO_MS.saturating_mul(1_000);
    let overhead_micros = local_overhead_ms.saturating_mul(1_000);
    let maximum_micros = limit_micros.saturating_sub(overhead_micros);
    let maximum_permitted_ms = (maximum_micros / 1_000).min(request_timeout_ms.saturating_sub(1));
    let per_shaping = [
        QuestionShaping::SharedStateQuestions,
        QuestionShaping::PerCandidateRequest,
    ]
    .into_iter()
    .map(|shaping| {
        let cell = cells.iter().find(|cell| {
            cell.plan.shaping == shaping && cell.plan.candidate_count == largest_candidate_count
        });
        let eligible = cell.filter(|cell| {
            cell.plan.measurement_status == "measured"
                && cell.plan.rate_limit_feasible
                && cell.failed_invocations == 0
                && cell.end_to_end_latency.p95.sample_count >= MIN_SAMPLES_FOR_P95
        });
        let p95 = eligible.and_then(|cell| cell.end_to_end_latency.p95.value_micros);
        let deadline = p95.map(|value| value.div_ceil(1_000));
        let fits = deadline.is_some_and(|value| {
            value.saturating_add(local_overhead_ms) <= MAX_ADDED_TIME_TO_FIRST_AUDIO_MS
                && value < request_timeout_ms
        });
        ShapingDeadline {
            shaping,
            candidate_count: largest_candidate_count,
            deadline_ms: fits.then_some(deadline).flatten(),
            fits,
            status: if fits {
                "measured_fit"
            } else if cell.is_none() {
                "missing_cell"
            } else if cell.is_some_and(|cell| cell.failed_invocations > 0) {
                "failed_invocations"
            } else if cell
                .is_some_and(|cell| cell.plan.measurement_status == "derived_not_measured")
            {
                "derived_not_measured"
            } else if cell
                .is_some_and(|cell| cell.end_to_end_latency.p95.sample_count < MIN_SAMPLES_FOR_P95)
            {
                "insufficient_successful_samples_for_p95"
            } else {
                "does_not_fit"
            }
            .to_owned(),
        }
    })
    .collect::<Vec<_>>();
    let fitting = per_shaping
        .iter()
        .filter_map(|result| {
            let deadline = result.deadline_ms?;
            let count = cells
                .iter()
                .find(|cell| {
                    cell.plan.shaping == result.shaping
                        && cell.plan.candidate_count == largest_candidate_count
                })?
                .end_to_end_latency
                .p95
                .sample_count;
            Some((deadline, count))
        })
        .max_by_key(|(deadline, _)| *deadline);
    let largest_fits = fitting.is_some();
    let largest_fitting_store_by_shaping = [
        QuestionShaping::SharedStateQuestions,
        QuestionShaping::PerCandidateRequest,
    ]
    .into_iter()
    .filter_map(|shaping| {
        cells
            .iter()
            .filter(|cell| cell.plan.shaping == shaping)
            .filter_map(|cell| {
                let p95 = cell.end_to_end_latency.p95.value_micros?;
                let deadline_ms = p95.div_ceil(1_000);
                (cell.plan.measurement_status == "measured"
                    && cell.plan.rate_limit_feasible
                    && cell.stop.is_none()
                    && cell.failed_invocations == 0
                    && cell.end_to_end_latency.p95.sample_count >= MIN_SAMPLES_FOR_P95
                    && deadline_ms.saturating_add(local_overhead_ms)
                        <= MAX_ADDED_TIME_TO_FIRST_AUDIO_MS
                    && deadline_ms < request_timeout_ms)
                    .then_some(FittingStoreDeadline {
                        shaping,
                        candidate_count: cell.plan.candidate_count,
                        deadline_ms: deadline_ms.min(maximum_permitted_ms),
                        sample_count: cell.end_to_end_latency.p95.sample_count,
                    })
            })
            .max_by_key(|store| store.candidate_count)
    })
    .collect::<Vec<_>>();
    let largest_fitting_store = largest_fitting_store_by_shaping
        .iter()
        .max_by(|left, right| {
            left.candidate_count
                .cmp(&right.candidate_count)
                .then_with(|| right.deadline_ms.cmp(&left.deadline_ms))
        })
        .cloned();
    DerivedInjectionDeadline {
        deadline_ms: fitting.map(|(deadline, _)| deadline.min(maximum_permitted_ms)),
        maximum_permitted_ms,
        source_sample_count: fitting.map(|(_, count)| count),
        any_measured_shaping_fits: fitting.is_some(),
        any_shape_fits_largest_store: largest_fits,
        finding: fitting.is_none().then(|| {
            format!(
                "no shaping at the largest configured store has at least {MIN_SAMPLES_FOR_P95} successful, failure-free samples fitting the fixed {MAX_ADDED_TIME_TO_FIRST_AUDIO_MS} ms added-time limit after {local_overhead_ms} ms of stated local overhead and below request timeout {request_timeout_ms} ms"
            )
        }),
        per_shaping,
        largest_fitting_store_by_shaping,
        largest_fitting_store,
    }
}

fn with_derived_per_candidate_latency(
    mut reports: Vec<BenchCellReport>,
    max_concurrency: usize,
) -> Vec<BenchCellReport> {
    let per_attempt_source = reports
        .iter()
        .find(|cell| {
            cell.plan.shaping == QuestionShaping::PerCandidateRequest
                && cell.plan.measurement_status == "measured"
                && cell.failed_invocations == 0
                && cell.per_attempt_latency.p95.sample_count >= MIN_SAMPLES_FOR_P95
        })
        .and_then(|cell| cell.per_attempt_latency.p95.value_micros);
    for cell in &mut reports {
        if cell.plan.measurement_status == "derived_not_measured" {
            cell.derived_end_to_end_p95_micros = per_attempt_source.map(|latency| {
                latency.saturating_mul(
                    (cell.plan.candidate_count as u64).div_ceil(max_concurrency.max(1) as u64),
                )
            });
        }
    }
    reports
}

fn aggregate_cell(
    plan: BenchCellPlan,
    samples: Vec<BenchSample>,
    wall_time: Duration,
    local_overhead_ms: u64,
    conversation_turns: u64,
    price: Option<&ModelPrice>,
) -> BenchCellReport {
    let latency = samples
        .iter()
        .filter(|sample| sample.failure.is_none())
        .map(|sample| sample.end_to_end_latency_micros)
        .collect::<Vec<_>>();
    let failure_histogram = samples
        .iter()
        .filter_map(|sample| sample.failure.as_ref())
        .fold(BTreeMap::<String, u64>::new(), |mut histogram, failure| {
            *histogram.entry(failure.clone()).or_default() += 1;
            histogram
        });
    let failed_invocations = failure_histogram.values().copied().sum();
    let attempt_latency = samples
        .iter()
        .filter(|sample| sample.failure.is_none())
        .flat_map(|sample| sample.attempt_latency_micros.iter().copied())
        .collect::<Vec<_>>();
    let usage_samples = samples
        .iter()
        .filter_map(|sample| {
            Some((
                sample.input_tokens?,
                sample.output_tokens?,
                sample.successful_provider_responses,
            ))
        })
        .collect::<Vec<_>>();
    let input_tokens = usage_samples.iter().map(|usage| usage.0).sum::<u64>();
    let output_tokens = usage_samples.iter().map(|usage| usage.1).sum::<u64>();
    let observed_requests = usage_samples.iter().map(|usage| usage.2).sum();
    let total_cost =
        price.and_then(|price| price_cost_nano_usd(input_tokens, output_tokens, price));
    let cost = total_cost.map(|total_nano_usd| MeasuredCost {
        total_nano_usd,
        observed_turns: usage_samples.len() as u64,
        cost_per_turn_nano_usd: div_round_nearest(total_nano_usd, usage_samples.len() as u64),
        conversation_turns,
        cost_per_conversation_nano_usd: div_round_nearest(
            total_nano_usd.saturating_mul(conversation_turns),
            usage_samples.len() as u64,
        ),
    });
    let cost_status = if usage_samples.is_empty() {
        "no_observed_usage"
    } else if price.is_none() {
        "tokens_only_unpriced_model"
    } else {
        "measured_from_observed_usage_and_local_price_table"
    }
    .to_owned();
    let usage = UsageSummary {
        observed_turns: usage_samples.len() as u64,
        observed_requests,
        total_input_tokens: input_tokens,
        total_output_tokens: output_tokens,
        mean_input_tokens_per_request: average_rounded(input_tokens, observed_requests),
        mean_output_tokens_per_request: average_rounded(output_tokens, observed_requests),
        cost,
        cost_status,
    };
    let retried = samples
        .iter()
        .filter(|sample| u64::from(sample.attempt_count) > plan.requests_per_turn)
        .count() as u64;
    let observed_attempt_count = samples
        .iter()
        .map(|sample| u64::from(sample.attempt_count))
        .sum();
    let p95 = percentile_set(&latency).p95;
    let overhead_micros = local_overhead_ms.saturating_mul(1_000);
    let projected = p95
        .value_micros
        .map(|value| value.saturating_add(overhead_micros));
    let p95_fits = if failed_invocations > 0 {
        None
    } else {
        projected.map(|value| value <= MAX_ADDED_TIME_TO_FIRST_AUDIO_MS * 1_000)
    };
    BenchCellReport {
        plan,
        sample_count: samples.len(),
        failed_invocations,
        failure_histogram,
        stop: None,
        derived_end_to_end_p95_micros: None,
        end_to_end_latency: percentile_set(&latency),
        per_attempt_latency: percentile_set(&attempt_latency),
        retried_invocation_count: retried,
        retry_incidence_sample_count: samples.len() as u64,
        retry_incidence: RetryIncidence {
            retried_invocations: retried,
            invocations: samples.len() as u64,
        },
        observed_attempt_count,
        usage,
        p95_fits_added_time_limit: p95_fits,
        projected_added_time_p95_micros: projected,
        cell_wall_time_micros: wall_time.as_micros() as u64,
        samples,
    }
}

fn percentile_set(values: &[u64]) -> PercentileSet {
    PercentileSet {
        p50: percentile_estimate(values, 50),
        p95: percentile_estimate(values, 95),
        p99: percentile_estimate(values, 99),
    }
}

fn render_percentiles(percentiles: &PercentileSet) -> String {
    [&percentiles.p50, &percentiles.p95, &percentiles.p99]
        .into_iter()
        .map(|estimate| {
            let value = estimate
                .value_micros
                .map(|micros| format!("{} ms", micros as f64 / 1_000.0))
                .unwrap_or_else(|| estimate.status.clone());
            format!(
                "p{}={} (n={})",
                estimate.percentile, value, estimate.sample_count
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn percentile_estimate(values: &[u64], percentile: u8) -> PercentileEstimate {
    let withheld = (percentile == 99 && values.len() < MIN_SAMPLES_FOR_MEANINGFUL_P99)
        || (percentile == 95 && values.len() < MIN_SAMPLES_FOR_P95);
    let value = if withheld {
        None
    } else {
        nearest_rank_percentile(values, percentile)
    };
    let status = if values.is_empty() {
        "no_samples"
    } else if withheld {
        "insufficient_samples"
    } else {
        "descriptive_nearest_rank_estimate"
    };
    PercentileEstimate {
        percentile,
        sample_count: values.len(),
        value_micros: value,
        status: status.to_owned(),
    }
}

fn sample_from_trace<T>(repetition: usize, traced: &Traced<T>) -> BenchSample {
    let (
        resolved_model_ids,
        attempt_count,
        retry_reasons,
        attempt_latencies,
        usage,
        successful_provider_responses,
    ) = match &traced.trace.service {
        Some(service) => (
            service.resolved_model_ids.clone(),
            service.attempt_count,
            service.retry_reasons.clone(),
            service.attempt_latency_micros.clone(),
            service
                .usage_parsed
                .as_ref()
                .map(|usage| (usage.input_tokens, usage.output_tokens)),
            match &service.usage_raw {
                Some(serde_json::Value::Array(values)) => values.len() as u64,
                Some(_) => 1,
                None => u64::from(service.usage_parsed.is_some()),
            },
        ),
        None => (
            vec![traced.trace.model_identity.identity_value.clone()],
            0,
            Vec::new(),
            Vec::new(),
            None,
            0,
        ),
    };
    BenchSample {
        repetition,
        invocation_id: traced.trace.invocation_id.clone(),
        resolved_model_ids,
        end_to_end_latency_micros: traced.trace.latency_micros,
        attempt_latency_micros: attempt_latencies,
        attempt_count,
        successful_provider_responses,
        max_input_tokens_per_request: traced.trace.service.as_ref().and_then(|service| {
            let values = match service.usage_raw.as_ref()? {
                serde_json::Value::Array(values) => values.iter().collect::<Vec<_>>(),
                value => vec![value],
            };
            values
                .into_iter()
                .filter_map(|value| value.get("input_tokens")?.as_u64())
                .max()
        }),
        retry_reasons,
        input_tokens: usage.map(|usage| usage.0),
        output_tokens: usage.map(|usage| usage.1),
        failure: traced.trace.failure.as_ref().map(ToString::to_string),
    }
}

fn build_findings(
    cells: &[BenchCellReport],
    deadline: &DerivedInjectionDeadline,
    backend: &BenchBackendSettings,
    largest_count: usize,
) -> Vec<String> {
    let mut findings = cells
        .iter()
        .filter(|cell| !cell.plan.rate_limit_feasible)
        .map(|cell| {
            format!(
                "{} shaping at {} candidates is infeasible by rate-limit arithmetic: {}",
                shaping_name(cell.plan.shaping),
                cell.plan.candidate_count,
                cell.plan
                    .infeasibility_basis
                    .as_deref()
                    .unwrap_or("rate limit")
            )
        })
        .collect::<Vec<_>>();
    if let Some(finding) = &deadline.finding {
        findings.push(finding.clone());
    } else if !deadline.any_shape_fits_largest_store {
        findings.push(format!(
            "no measured shaping fits the fixed {MAX_ADDED_TIME_TO_FIRST_AUDIO_MS} ms limit at the largest configured store size ({largest_count} candidates)"
        ));
    }
    if !deadline.any_shape_fits_largest_store {
        findings.push(match &deadline.largest_fitting_store {
            Some(store) => format!(
                "largest store that fits the fixed {MAX_ADDED_TIME_TO_FIRST_AUDIO_MS} ms limit is {} candidates with {} shaping; the largest configured store ({largest_count} candidates) does not",
                store.candidate_count,
                shaping_name(store.shaping)
            ),
            None => format!(
                "no measured store fits the fixed {MAX_ADDED_TIME_TO_FIRST_AUDIO_MS} ms limit; the largest configured store ({largest_count} candidates) does not"
            ),
        });
    }
    if backend.backend_kind == BackendKind::Fixture {
        findings.push(
            "fixture results are synthetic, carry no hosted usage, and are not relevance evidence"
                .to_owned(),
        );
    }
    findings
}

fn synthetic_request(
    candidates: &[Candidate],
    repetition: usize,
    shaping: QuestionShaping,
) -> PairScoreRequest {
    let utterance = synthetic_utterance(repetition);
    PairScoreRequest {
        task: RelevanceTask::MemoryRelevance,
        utterance_content_hash: sha256(utterance.as_bytes()),
        utterance_text: utterance,
        candidates: candidates.to_vec(),
        options: PairScoringOptions {
            question_wording_version: crate::backends::remote_http::QUESTION_WORDING_VERSION
                .to_owned(),
            shaping,
            score_kind_expected: ScoreKind::Probability,
        },
    }
}

fn synthetic_candidates(count: usize) -> Vec<Candidate> {
    const EXAMPLES: &[(&str, &str)] = &[
        (
            "Garden planning",
            "The person prefers growing vegetables in raised beds and keeps a simple seasonal planting schedule.",
        ),
        (
            "Cycling routes",
            "The person enjoys quieter roads and plans weekend rides around the lake before lunch.",
        ),
        (
            "Bread notes",
            "The person is learning to bake sourdough and adjusts the dough hydration after each attempt.",
        ),
        (
            "Reading list",
            "The person saves thoughtful science fiction novels and usually reads a chapter in the evening.",
        ),
        (
            "Workshop layout",
            "The person keeps frequently used hand tools near the workbench and labels storage boxes clearly.",
        ),
        (
            "Train journeys",
            "The person compares rail routes early and prefers trips with one comfortable transfer.",
        ),
        (
            "Music practice",
            "The person practices piano in short sessions and slows difficult passages before increasing tempo.",
        ),
        (
            "Tea preferences",
            "The person likes lightly oxidized tea and uses a timer to keep each steep consistent.",
        ),
        (
            "Trail notes",
            "The person chooses forest walks with a clear path and checks daylight before setting out.",
        ),
        (
            "Budget routine",
            "The person reviews household spending once a month and keeps recurring costs in a small ledger.",
        ),
        (
            "Photo archive",
            "The person sorts travel photographs by year and adds short captions to memorable places.",
        ),
        (
            "Plant care",
            "The person groups indoor plants by their light needs and waters them after checking the soil.",
        ),
        (
            "Museum visits",
            "The person enjoys small history museums and leaves time to read the exhibit notes carefully.",
        ),
        (
            "Meal preparation",
            "The person plans a few flexible dinners and uses leftover ingredients before shopping again.",
        ),
        (
            "Language study",
            "The person learns new vocabulary by reading short passages and reviewing useful phrases aloud.",
        ),
        (
            "Desk setup",
            "The person prefers a clear desk, warm task lighting, and a notebook beside the keyboard.",
        ),
        (
            "Bird observations",
            "The person records seasonal bird sightings and notes the habitat where each one appeared.",
        ),
        (
            "Home repairs",
            "The person keeps basic repair supplies together and writes down measurements before buying replacements.",
        ),
        (
            "Sleep routine",
            "The person winds down with quiet reading and keeps bright screens away from the bedside.",
        ),
        (
            "Coastal visits",
            "The person enjoys visiting the shore at low tide and looks for sheltered walking paths.",
        ),
        (
            "Archive research",
            "The person follows public records and keeps source references beside each research note.",
        ),
        (
            "Coffee method",
            "The person makes pour-over coffee slowly and changes the grind when the cup tastes uneven.",
        ),
        (
            "Community garden",
            "The person shares basic gardening tasks with neighbors and keeps the common paths tidy.",
        ),
        (
            "Board games",
            "The person likes cooperative strategy games and explains the rules with a short practice round.",
        ),
        (
            "Winter clothing",
            "The person layers light clothing for cold walks and keeps spare gloves in a day bag.",
        ),
    ];
    (0..count)
        .map(|index| {
            let (title, summary) = EXAMPLES[index % EXAMPLES.len()];
            let cycle = index / EXAMPLES.len();
            let text = if cycle == 0 {
                format!("{title} — {summary}")
            } else {
                format!("{title}, note set {cycle} — {summary}")
            };
            Candidate {
                candidate_id: format!("synthetic-memory-{index:04}"),
                candidate_content_hash: sha256(text.as_bytes()),
                candidate_text: text,
                candidate_kind: CandidateKind("memory".to_owned()),
            }
        })
        .collect()
}

fn synthetic_utterance(index: usize) -> String {
    const EXAMPLES: &[&str] = &[
        "I want to make the balcony useful for growing fresh food through the colder months.",
        "Could we find a calmer way to spend a Saturday outdoors without driving very far?",
        "I am trying to get more consistent at a difficult skill by practicing in shorter blocks.",
        "How can I keep household plans organized without making the routine feel complicated?",
        "I would like to preserve notes from places I visit so I can find them again later.",
        "What is a comfortable way to prepare something warm before starting the morning?",
    ];
    EXAMPLES[index % EXAMPLES.len()].to_owned()
}

fn pico_usd_per_token(price_per_million: &str) -> Option<u128> {
    let (whole, fraction) = price_per_million
        .split_once('.')
        .unwrap_or((price_per_million, ""));
    if fraction.len() > 9
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let scale = 10_u128.checked_pow(fraction.len() as u32)?;
    let whole = whole.parse::<u128>().ok()?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u128>().ok()?
    };
    let numerator = whole
        .checked_mul(scale)?
        .checked_add(fraction)?
        .checked_mul(1_000_000)?;
    (numerator % scale == 0).then_some(numerator / scale)
}

fn div_round_nearest(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }
    numerator
        .saturating_add(denominator / 2)
        .checked_div(denominator)
        .unwrap_or_default()
}

fn average_rounded(total: u64, count: u64) -> Option<u64> {
    (count != 0).then(|| div_round_nearest(total, count))
}

fn shaping_name(shaping: QuestionShaping) -> &'static str {
    match shaping {
        QuestionShaping::SharedStateQuestions => "shared_state_questions",
        QuestionShaping::PerCandidateRequest => "per_candidate_request",
    }
}

fn machine_environment(network_description: &str) -> MachineEnvironment {
    let machine_name = env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_owned());
    let code_commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    let code_tree_dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| !output.stdout.is_empty());
    MachineEnvironment {
        machine_name,
        operating_system: env::consts::OS.to_owned(),
        architecture: env::consts::ARCH.to_owned(),
        logical_cpu_count: std::thread::available_parallelism().ok().map(usize::from),
        network_description: if network_description.trim().is_empty() {
            "not supplied by operator".to_owned()
        } else {
            network_description.to_owned()
        },
        code_commit,
        code_tree_dirty,
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), BenchError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes).map_err(|source| BenchError::Artifact {
        path: path.to_path_buf(),
        source,
    })
}

fn format_usd_nano(nano_usd: u64) -> String {
    format!(
        "USD {}.{:09}",
        nano_usd / 1_000_000_000,
        nano_usd % 1_000_000_000
    )
}

/// Makes a timestamped run id with a random suffix.
pub fn default_run_id() -> String {
    let format = time::format_description::parse("[year][month][day]-[hour][minute][second]")
        .expect("static timestamp format");
    let timestamp = OffsetDateTime::now_utc()
        .format(&format)
        .unwrap_or_else(|_| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string()
        });
    format!(
        "{timestamp}-relevance-bench-{}",
        uuid::Uuid::new_v4().simple()
    )
}

/// Resolves a run directory beneath its selected output root.
pub fn run_directory(root: &Path, run_id: &str) -> PathBuf {
    root.join(run_id)
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::net::TcpListener;

    use crate::{
        backends::remote_http::{RemoteRelevanceJudge, RemoteRelevanceJudgeConfig},
        pair_scoring::{PairScorer, PairScoringService},
        trace::SemanticFailure,
    };

    use super::*;

    fn options(directory: PathBuf) -> BenchOptions {
        BenchOptions {
            candidate_counts: vec![18, 100, 500],
            repetitions: 1,
            per_candidate_repetitions: 1,
            max_total_requests: DEFAULT_MAX_TOTAL_REQUESTS,
            target_turns_per_minute: 3,
            judge_workloads_per_turn: 2,
            conversation_turns: 10,
            local_overhead_ms: 0,
            network_description: "local test network".to_owned(),
            run_directory: directory,
            run_id: "test-bench".to_owned(),
            dry_run: false,
        }
    }

    fn remote_settings() -> BenchBackendSettings {
        BenchBackendSettings {
            backend_name: "remote_http".to_owned(),
            pinned_model_id: "jev-1.13.0".to_owned(),
            endpoint: "https://api.typesafe.ai/v1/systemone".to_owned(),
            request_timeout: Duration::from_secs(2),
            max_attempts: 3,
            max_concurrency: 4,
            initial_backoff: Duration::from_millis(20),
            max_backoff: Duration::from_millis(80),
            configured_injection_deadline: Duration::from_millis(200),
            backend_kind: BackendKind::RemoteHttp,
        }
    }

    #[test]
    fn nearest_rank_percentiles_use_expected_order_statistics() {
        assert_eq!(nearest_rank_percentile(&[9, 1, 5, 3, 7], 50), Some(5));
        assert_eq!(nearest_rank_percentile(&[9, 1, 5, 3, 7], 95), Some(9));
        assert_eq!(nearest_rank_percentile(&[1, 2], 0), None);
        assert_eq!(nearest_rank_percentile(&[], 99), None);
    }

    #[test]
    fn p99_is_withheld_below_one_hundred_samples() {
        let estimates = percentile_set(&[1_000, 2_000, 3_000]);
        assert_eq!(estimates.p50.value_micros, Some(2_000));
        assert_eq!(estimates.p95.sample_count, 3);
        assert_eq!(estimates.p99.value_micros, None);
        assert_eq!(estimates.p99.status, "insufficient_samples");
        let full = percentile_set(&(1..=100).collect::<Vec<_>>());
        assert_eq!(full.p99.value_micros, Some(99));
        assert_eq!(full.p99.sample_count, 100);
    }

    #[test]
    fn observed_usage_cost_uses_table_and_unpriced_model_is_tokens_only() {
        let priced_table = price_table_identity("jev-1.13.0").expect("table");
        let price = priced_table.pinned_model_price.expect("jev price");
        assert_eq!(price_cost_nano_usd(1_000_000, 50, &price), Some(42_000_000));
        assert!(
            price_table_identity("jev-latest")
                .expect("table")
                .pinned_model_price
                .is_none()
        );
        let plan = plan_run(&options(PathBuf::from("runs/test")), &remote_settings())
            .expect("plan")
            .cells[0]
            .clone();
        let sample = BenchSample {
            repetition: 0,
            invocation_id: "remote-1".to_owned(),
            resolved_model_ids: vec!["jev-1.13.0".to_owned()],
            end_to_end_latency_micros: 10_000,
            attempt_latency_micros: vec![9_000],
            attempt_count: 1,
            successful_provider_responses: 1,
            max_input_tokens_per_request: Some(100),
            retry_reasons: Vec::new(),
            input_tokens: Some(100),
            output_tokens: Some(25),
            failure: None,
        };
        let priced = aggregate_cell(
            plan.clone(),
            vec![sample.clone()],
            Duration::from_millis(1),
            0,
            10,
            Some(&price),
        );
        assert_eq!(priced.usage.total_input_tokens, 100);
        assert_eq!(priced.usage.cost.as_ref().unwrap().total_nano_usd, 4_200);
        assert_eq!(
            priced
                .usage
                .cost
                .as_ref()
                .unwrap()
                .cost_per_conversation_nano_usd,
            42_000
        );
        let unpriced = aggregate_cell(plan, vec![sample], Duration::from_millis(1), 0, 10, None);
        assert_eq!(unpriced.usage.total_input_tokens, 100);
        assert_eq!(unpriced.usage.cost, None);
        assert_eq!(unpriced.usage.cost_status, "tokens_only_unpriced_model");
    }

    #[test]
    fn insufficient_p95_cannot_set_deadline() {
        let cell = fake_cell(1_000, MIN_SAMPLES_FOR_P95 - 1);
        assert_eq!(cell.end_to_end_latency.p95.value_micros, None);
        let deadline = derive_injection_deadline(&[cell], 18, 0, 1_000);
        assert_eq!(deadline.deadline_ms, None);
        assert_eq!(
            deadline.per_shaping[0].status,
            "insufficient_successful_samples_for_p95"
        );
    }

    #[test]
    fn body_based_token_limit_risk_is_reported_in_plan() {
        let plan =
            plan_run(&options(PathBuf::from("runs/test")), &remote_settings()).expect("plan");
        let largest = plan
            .cells
            .iter()
            .find(|cell| {
                cell.shaping == QuestionShaping::SharedStateQuestions && cell.candidate_count == 500
            })
            .expect("cell");
        assert!(largest.heuristic_token_limit_risk);
        assert!(largest.estimated_max_input_tokens_per_request > 0);
        assert!(largest.estimated_max_state_and_question_tokens > 0);
    }

    #[test]
    fn default_remote_sampling_plan_fits_nominal_cap() {
        let mut configured = options(PathBuf::from("runs/test"));
        configured.repetitions = DEFAULT_REPETITIONS;
        configured.per_candidate_repetitions = DEFAULT_PER_CANDIDATE_REPETITIONS;
        configured.target_turns_per_minute = DEFAULT_TARGET_TURNS_PER_MINUTE;
        let plan = plan_run(&configured, &remote_settings()).expect("plan");
        assert_eq!(plan.planned_nominal_total_requests, 301);
        assert_eq!(plan.planned_worst_case_total_requests, 903);
        assert!(plan.planned_nominal_total_requests <= DEFAULT_MAX_TOTAL_REQUESTS);
    }

    #[test]
    fn low_nonzero_price_remains_priced() {
        let price = ModelPrice {
            model_id: "low".to_owned(),
            input_usd_per_million_tokens: "0.0001".to_owned(),
            output_usd_per_million_tokens: "0".to_owned(),
        };
        assert_eq!(price_cost_nano_usd(10_000, 0, &price), Some(1_000));
    }

    #[test]
    fn partial_failed_turn_counts_only_successful_provider_responses() {
        let mut plan = plan_run(&options(PathBuf::from("runs/test")), &remote_settings())
            .expect("plan")
            .cells
            .into_iter()
            .find(|cell| {
                cell.shaping == QuestionShaping::PerCandidateRequest && cell.candidate_count == 18
            })
            .expect("cell");
        plan.repetitions = 1;
        let sample = BenchSample {
            repetition: 0,
            invocation_id: "partial".to_owned(),
            resolved_model_ids: vec!["jev-1.13.0".to_owned()],
            end_to_end_latency_micros: 1_000,
            attempt_latency_micros: vec![100; 18],
            attempt_count: 18,
            successful_provider_responses: 17,
            max_input_tokens_per_request: Some(10),
            retry_reasons: Vec::new(),
            input_tokens: Some(170),
            output_tokens: Some(17),
            failure: Some("unauthorized".to_owned()),
        };
        let cell = aggregate_cell(plan, vec![sample], Duration::ZERO, 0, 10, None);
        assert_eq!(cell.usage.observed_requests, 17);
        assert_eq!(cell.usage.mean_input_tokens_per_request, Some(10));
        assert_eq!(cell.failed_invocations, 1);
        assert_eq!(cell.end_to_end_latency.p50.value_micros, None);
    }

    #[test]
    fn larger_per_candidate_latency_is_derived_from_attempts_and_waves() {
        let mut source = fake_cell(2_000, MIN_SAMPLES_FOR_P95);
        source.plan.shaping = QuestionShaping::PerCandidateRequest;
        source.plan.candidate_count = 18;
        let mut target = fake_cell(0, 0);
        target.plan.shaping = QuestionShaping::PerCandidateRequest;
        target.plan.candidate_count = 100;
        target.plan.measurement_status = "derived_not_measured".to_owned();
        let reports = with_derived_per_candidate_latency(vec![target, source], 4);
        assert_eq!(reports[0].derived_end_to_end_p95_micros, Some(50_000));
        assert_eq!(reports[0].end_to_end_latency.p95.value_micros, None);
    }

    #[test]
    fn plan_runs_smallest_store_first_within_each_shaping() {
        let plan =
            plan_run(&options(PathBuf::from("runs/test")), &remote_settings()).expect("plan");
        assert_eq!(
            plan.cells
                .iter()
                .map(|cell| (cell.shaping, cell.candidate_count))
                .collect::<Vec<_>>(),
            vec![
                (QuestionShaping::SharedStateQuestions, 18),
                (QuestionShaping::SharedStateQuestions, 100),
                (QuestionShaping::SharedStateQuestions, 500),
                (QuestionShaping::PerCandidateRequest, 18),
                (QuestionShaping::PerCandidateRequest, 100),
                (QuestionShaping::PerCandidateRequest, 500),
            ]
        );
        assert_eq!(plan.cells[3].measurement_status, "measured");
        assert_eq!(plan.cells[4].measurement_status, "derived_not_measured");
        assert!(render_plan(&plan).contains("stops after 3 consecutive failed invocations"));
    }

    #[test]
    fn smaller_fitting_store_does_not_change_largest_store_verdict() {
        let mut small = fake_cell(100_000, MIN_SAMPLES_FOR_P95);
        small.plan.candidate_count = 18;
        let mut large = fake_cell(100_000, MIN_SAMPLES_FOR_P95);
        large.plan.candidate_count = 500;
        large.failed_invocations = 1;
        let derived = derive_injection_deadline(&[small, large], 500, 0, 1_000);
        assert_eq!(derived.deadline_ms, None);
        assert!(!derived.any_shape_fits_largest_store);
        assert_eq!(derived.per_shaping[0].status, "failed_invocations");
        assert_eq!(derived.largest_fitting_store_by_shaping.len(), 1);
        let fitting = derived.largest_fitting_store.as_ref().expect("small fit");
        assert_eq!(fitting.candidate_count, 18);
        assert_eq!(fitting.deadline_ms, 100);
        assert_eq!(fitting.sample_count, MIN_SAMPLES_FOR_P95);
        let findings = build_findings(&[], &derived, &remote_settings(), 500);
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("18 candidates"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("(500 candidates) does not"))
        );
    }

    #[test]
    fn no_fitting_store_is_reported_as_none() {
        let mut cell = fake_cell(301_000, MIN_SAMPLES_FOR_P95);
        cell.plan.candidate_count = 500;
        let derived = derive_injection_deadline(&[cell], 500, 0, 1_000);
        assert_eq!(derived.deadline_ms, None);
        assert!(derived.largest_fitting_store_by_shaping.is_empty());
        assert_eq!(derived.largest_fitting_store, None);
    }

    #[test]
    fn equal_sized_fitting_stores_choose_lower_deadline() {
        let shared = fake_cell(150_000, MIN_SAMPLES_FOR_P95);
        let mut per_candidate = fake_cell(100_000, MIN_SAMPLES_FOR_P95);
        per_candidate.plan.shaping = QuestionShaping::PerCandidateRequest;
        let derived = derive_injection_deadline(&[shared, per_candidate], 18, 0, 1_000);
        assert_eq!(derived.largest_fitting_store_by_shaping.len(), 2);
        let best = derived.largest_fitting_store.expect("best fit");
        assert_eq!(best.shaping, QuestionShaping::PerCandidateRequest);
        assert_eq!(best.deadline_ms, 100);
    }

    #[test]
    fn aggregation_counts_retried_invocations_and_costs_observed_turns() {
        let price = price_table_identity("jev-1.13.0")
            .expect("table")
            .pinned_model_price
            .expect("price");
        let mut plan = plan_run(&options(PathBuf::from("runs/test")), &remote_settings())
            .expect("plan")
            .cells[0]
            .clone();
        plan.repetitions = 2;
        let samples = vec![
            BenchSample {
                repetition: 0,
                invocation_id: "retried".to_owned(),
                resolved_model_ids: vec!["jev-1.13.0".to_owned()],
                end_to_end_latency_micros: 30_000,
                attempt_latency_micros: vec![10_000, 18_000],
                attempt_count: 2,
                successful_provider_responses: 1,
                max_input_tokens_per_request: Some(100),
                retry_reasons: vec!["rate_limited".to_owned()],
                input_tokens: Some(100),
                output_tokens: Some(0),
                failure: None,
            },
            BenchSample {
                repetition: 1,
                invocation_id: "single-attempt".to_owned(),
                resolved_model_ids: vec!["jev-1.13.0".to_owned()],
                end_to_end_latency_micros: 20_000,
                attempt_latency_micros: vec![19_000],
                attempt_count: 1,
                successful_provider_responses: 1,
                max_input_tokens_per_request: Some(100),
                retry_reasons: Vec::new(),
                input_tokens: Some(100),
                output_tokens: Some(0),
                failure: None,
            },
        ];
        let report = aggregate_cell(
            plan,
            samples,
            Duration::from_millis(50),
            10,
            10,
            Some(&price),
        );
        assert_eq!(report.retry_incidence.retried_invocations, 1);
        assert_eq!(report.retry_incidence.invocations, 2);
        assert_eq!(report.per_attempt_latency.p50.value_micros, Some(18_000));
        assert_eq!(report.usage.total_input_tokens, 200);
        assert_eq!(report.usage.observed_requests, 2);
        let cost = report.usage.cost.expect("priced usage");
        assert_eq!(cost.total_nano_usd, 8_400);
        assert_eq!(cost.cost_per_turn_nano_usd, 4_200);
        assert_eq!(cost.cost_per_conversation_nano_usd, 42_000);
    }

    #[tokio::test]
    async fn request_budget_refusal_writes_plan_and_calls_no_backend() {
        let temp = tempdir().expect("temp");
        let run_dir = temp.path().join("refused");
        let mut configured = options(run_dir.clone());
        configured.max_total_requests = 10;
        let service = CountingService(Arc::new(AtomicUsize::new(0)));
        let result = run_with_service(configured, remote_settings(), &service).await;
        assert!(matches!(
            result,
            Err(BenchError::RequestBudgetExceeded { .. })
        ));
        assert_eq!(service.0.load(Ordering::SeqCst), 0);
        let plan: BenchPlan =
            serde_json::from_slice(&fs::read(run_dir.join("bench-plan.json")).expect("plan file"))
                .expect("parse plan");
        assert_eq!(plan.planned_worst_case_total_requests, 66);
        assert_eq!(plan.planned_nominal_total_requests, 22);
        assert!(plan.cells.iter().any(|cell| {
            cell.shaping == QuestionShaping::PerCandidateRequest
                && cell.candidate_count == 500
                && !cell.rate_limit_feasible
                && cell.planned_worst_case_requests == 0
        }));
    }

    #[tokio::test]
    async fn dry_run_persists_plan_and_sends_nothing() {
        let temp = tempdir().expect("temp");
        let run_dir = temp.path().join("dry");
        let mut configured = options(run_dir.clone());
        configured.dry_run = true;
        let service = CountingService(Arc::new(AtomicUsize::new(0)));
        let outcome = run_with_service(configured, remote_settings(), &service)
            .await
            .expect("dry run");
        assert!(matches!(outcome, BenchOutcome::DryRun { .. }));
        assert_eq!(service.0.load(Ordering::SeqCst), 0);
        assert!(run_dir.join("bench-plan.json").exists());
        assert!(!run_dir.join("bench-report.json").exists());
    }

    #[tokio::test]
    async fn failed_cell_stops_after_three_and_next_cell_runs() {
        let temp = tempdir().expect("temp");
        let mut configured = options(temp.path().join("stopped"));
        configured.candidate_counts = vec![2, 1];
        configured.repetitions = 6;
        let service = ScriptedService::new((0..100).collect());
        let outcome = run_with_service(configured, remote_settings(), &service)
            .await
            .expect("bench");
        let BenchOutcome::Completed { report_path } = outcome else {
            panic!("report");
        };
        let report: BenchReport =
            serde_json::from_slice(&fs::read(report_path).expect("report file"))
                .expect("parse report");
        assert_eq!(report.cells[0].sample_count, 3);
        assert_eq!(report.cells[0].failed_invocations, 3);
        let stop = report.cells[0].stop.as_ref().expect("early stop");
        assert_eq!(stop.status, "stopped_after_consecutive_failures");
        assert_eq!(stop.consecutive_failures, 3);
        assert_eq!(
            stop.last_failure_reason,
            SemanticFailure::Timeout.to_string()
        );
        assert_eq!(report.cells[0].p95_fits_added_time_limit, None);
        assert_eq!(report.cells[1].sample_count, 3);
        assert!(report.cells[1].stop.is_some());
        assert_eq!(service.calls.load(Ordering::SeqCst), 8);
    }

    #[tokio::test]
    async fn success_resets_consecutive_failure_count() {
        let temp = tempdir().expect("temp");
        let mut configured = options(temp.path().join("reset"));
        configured.candidate_counts = vec![1];
        configured.repetitions = 6;
        let service = ScriptedService::new(vec![1, 2, 4, 5, 6]);
        let outcome = run_with_service(configured, remote_settings(), &service)
            .await
            .expect("bench");
        let BenchOutcome::Completed { report_path } = outcome else {
            panic!("report");
        };
        let report: BenchReport =
            serde_json::from_slice(&fs::read(report_path).expect("report file"))
                .expect("parse report");
        assert_eq!(report.cells[0].sample_count, 6);
        assert_eq!(report.cells[0].failed_invocations, 5);
        assert_eq!(
            report.cells[0].stop.as_ref().unwrap().consecutive_failures,
            3
        );
        assert_eq!(service.calls.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn large_per_candidate_cell_is_infeasible_by_derived_rate_arithmetic() {
        let plan =
            plan_run(&options(PathBuf::from("runs/test")), &remote_settings()).expect("plan");
        let large = plan
            .cells
            .iter()
            .find(|cell| {
                cell.shaping == QuestionShaping::PerCandidateRequest && cell.candidate_count == 500
            })
            .expect("large cell");
        assert!(!large.rate_limit_feasible);
        assert_eq!(large.planned_worst_case_requests, 0);
        assert!(
            large
                .infeasibility_basis
                .as_ref()
                .unwrap()
                .contains("maximum is 1.20")
        );
    }

    #[test]
    fn moving_model_alias_is_rejected_for_remote_measurement() {
        let mut backend = remote_settings();
        backend.pinned_model_id = "jev-latest".to_owned();
        assert!(matches!(
            plan_run(&options(PathBuf::from("runs/test")), &backend),
            Err(BenchError::InvalidOptions(_))
        ));
    }

    #[test]
    fn derived_injection_deadline_never_exceeds_fixed_added_time() {
        let cell = fake_cell(500_000, 100);
        for overhead in [0, 50, 250, 300, 500] {
            let derived =
                derive_injection_deadline(std::slice::from_ref(&cell), 18, overhead, 1_000);
            if let Some(deadline_ms) = derived.deadline_ms {
                assert!(deadline_ms <= MAX_ADDED_TIME_TO_FIRST_AUDIO_MS);
                assert!(deadline_ms.saturating_add(overhead) <= MAX_ADDED_TIME_TO_FIRST_AUDIO_MS);
            }
        }
        let slow = derive_injection_deadline(&[fake_cell(500_000, 4)], 18, 0, 1_000);
        assert_eq!(slow.deadline_ms, None);
        assert!(slow.finding.is_some());
        let invalid_timeout = derive_injection_deadline(&[fake_cell(250_000, 100)], 18, 0, 200);
        assert_eq!(invalid_timeout.deadline_ms, None);
        let valid_timeout = derive_injection_deadline(&[fake_cell(250_000, 100)], 18, 0, 251);
        assert_eq!(valid_timeout.deadline_ms, Some(250));
        assert!(valid_timeout.deadline_ms.unwrap() < 251);
    }

    #[tokio::test]
    async fn local_http_stub_serializes_and_parses_generated_report() {
        let requests = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/v1/systemone", post(stub_response))
            .with_state(requests.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address: SocketAddr = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("stub");
        });

        let temp = tempdir().expect("temp");
        let run_dir = temp.path().join("stub");
        let mut configured = options(run_dir.clone());
        configured.candidate_counts = vec![2];
        configured.max_total_requests = 20;
        let backend = BenchBackendSettings {
            endpoint: format!("http://{address}/v1/systemone"),
            max_attempts: 1,
            ..remote_settings()
        };
        let service = RemoteRelevanceJudge::new(RemoteRelevanceJudgeConfig {
            base_url: format!("http://{address}"),
            api_key: "stub-key".to_owned(),
            pinned_model_id: "jev-1.13.0".to_owned(),
            max_attempts: 1,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            request_timeout: Duration::from_secs(2),
            max_concurrency: 2,
        })
        .expect("judge");
        let outcome = run_with_service(configured, backend, &service)
            .await
            .expect("bench");
        let BenchOutcome::Completed { report_path } = outcome else {
            panic!("expected measurement report");
        };
        let report: BenchReport =
            serde_json::from_slice(&fs::read(report_path).expect("report file"))
                .expect("parse generated report");
        assert_eq!(report.backend, BackendKind::RemoteHttp);
        assert_eq!(report.pinned_model_id, "jev-1.13.0");
        assert_eq!(report.resolved_model_ids, ["jev-1.13.0-resolved"]);
        assert_eq!(report.question_wording_version, "relevance_noul_v1");
        assert_eq!(report.inputs.requests_per_minute, 1_200);
        assert_eq!(report.cells.len(), 2);
        assert!(report.cells.iter().all(|cell| cell.sample_count == 1));
        assert_eq!(report.cells[0].usage.total_input_tokens, 120);
        assert_eq!(report.cells[1].usage.total_input_tokens, 240);
        assert_eq!(report.cells[0].per_attempt_latency.p50.sample_count, 1);
        assert_eq!(report.cells[1].per_attempt_latency.p50.sample_count, 2);
        assert!(report.cells.iter().all(|cell| cell.usage.cost.is_some()));
        assert!(
            report
                .cells
                .iter()
                .all(|cell| cell.end_to_end_latency.p99.value_micros.is_none())
        );
        assert_eq!(requests.load(Ordering::SeqCst), 4);
        server.abort();
    }

    #[tokio::test]
    async fn fast_unauthorized_responses_never_produce_a_fit_verdict() {
        let app = Router::new().route("/v1/systemone", post(|| async { StatusCode::UNAUTHORIZED }));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("stub");
        });
        let temp = tempdir().expect("temp");
        let mut configured = options(temp.path().join("unauthorized"));
        configured.candidate_counts = vec![2];
        let backend = BenchBackendSettings {
            endpoint: format!("http://{address}/v1/systemone"),
            max_attempts: 1,
            ..remote_settings()
        };
        let judge = RemoteRelevanceJudge::new(RemoteRelevanceJudgeConfig {
            base_url: format!("http://{address}"),
            api_key: "wrong".to_owned(),
            pinned_model_id: "jev-1.13.0".to_owned(),
            max_attempts: 1,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            request_timeout: Duration::from_secs(2),
            max_concurrency: 2,
        })
        .expect("judge");
        let (judge, sent) = judge.with_request_budget(configured.max_total_requests);
        let outcome = run_with_service_counted(configured, backend, &judge, Some(sent))
            .await
            .expect("report");
        let BenchOutcome::Completed { report_path } = outcome else {
            panic!("report");
        };
        let report: BenchReport =
            serde_json::from_slice(&fs::read(report_path).expect("read")).expect("parse");
        assert!(report.cells.iter().all(|cell| cell.failed_invocations > 0));
        assert!(
            report
                .cells
                .iter()
                .all(|cell| cell.end_to_end_latency.p95.value_micros.is_none())
        );
        assert!(
            report
                .cells
                .iter()
                .all(|cell| cell.p95_fits_added_time_limit.is_none())
        );
        assert_eq!(report.derived_injection_deadline.deadline_ms, None);
        assert!(render_report_summary(&report).contains("failures=1/1"));
        server.abort();
    }

    #[tokio::test]
    async fn runtime_cap_stops_after_actual_sends_reach_limit() {
        async fn retry_once(
            State(counter): State<Arc<AtomicUsize>>,
            Json(request): Json<serde_json::Value>,
        ) -> (StatusCode, Json<serde_json::Value>) {
            let number = counter.fetch_add(1, Ordering::SeqCst);
            if number % 2 == 0 {
                return (StatusCode::TOO_MANY_REQUESTS, Json(json!({})));
            }
            let answers = request["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json!({"type":"noul","noul":0.5})))
                .collect::<serde_json::Map<_, _>>();
            (
                StatusCode::OK,
                Json(
                    json!({"model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":10,"output_tokens":1}}),
                ),
            )
        }
        let counter = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/v1/systemone", post(retry_once))
            .with_state(counter.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("stub");
        });
        let temp = tempdir().expect("temp");
        let mut configured = options(temp.path().join("cap"));
        configured.candidate_counts = vec![2];
        configured.repetitions = 3;
        configured.max_total_requests = 6;
        let backend = BenchBackendSettings {
            endpoint: format!("http://{address}/v1/systemone"),
            max_attempts: 2,
            ..remote_settings()
        };
        let judge = RemoteRelevanceJudge::new(RemoteRelevanceJudgeConfig {
            base_url: format!("http://{address}"),
            api_key: "stub".to_owned(),
            pinned_model_id: "jev-1.13.0".to_owned(),
            max_attempts: 2,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            request_timeout: Duration::from_secs(2),
            max_concurrency: 2,
        })
        .expect("judge");
        let (judge, sent) = judge.with_request_budget(6);
        let outcome = run_with_service_counted(configured, backend, &judge, Some(sent))
            .await
            .expect("report");
        let BenchOutcome::Completed { report_path } = outcome else {
            panic!("report");
        };
        let report: BenchReport =
            serde_json::from_slice(&fs::read(report_path).expect("read")).expect("parse");
        assert_eq!(report.actual_requests_sent, 6);
        assert_eq!(counter.load(Ordering::SeqCst), 6);
        assert!(report.stopped_at_request_cap);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.contains("stopped cleanly"))
        );
        server.abort();
    }

    #[derive(Clone)]
    struct CountingService(Arc<AtomicUsize>);

    impl PairScoringService for CountingService {
        fn score_pairs(
            &self,
            request: PairScoreRequest,
            _deadline: InjectionDeadline,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Traced<Vec<crate::PairScore>>> + Send + '_>,
        > {
            self.0.fetch_add(1, Ordering::SeqCst);
            let scorer = FixtureRelevanceJudge::new(FixtureRelevanceJudgeConfig::default());
            Box::pin(async move { scorer.score_pairs(request) })
        }
    }

    struct ScriptedService {
        calls: Arc<AtomicUsize>,
        failed_calls: Vec<usize>,
    }

    impl ScriptedService {
        fn new(failed_calls: Vec<usize>) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                failed_calls,
            }
        }
    }

    impl PairScoringService for ScriptedService {
        fn score_pairs(
            &self,
            request: PairScoreRequest,
            _deadline: InjectionDeadline,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Traced<Vec<crate::PairScore>>> + Send + '_>,
        > {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let fails = self.failed_calls.contains(&call);
            let scorer = FixtureRelevanceJudge::new(FixtureRelevanceJudgeConfig::default());
            Box::pin(async move {
                let mut traced = scorer.score_pairs(request);
                if fails {
                    traced.trace.failure = Some(SemanticFailure::Timeout);
                    traced.outcome = Err(SemanticFailure::Timeout);
                }
                traced
            })
        }
    }

    fn fake_cell(latency_micros: u64, count: usize) -> BenchCellReport {
        let plan = BenchCellPlan {
            shaping: QuestionShaping::SharedStateQuestions,
            candidate_count: 18,
            repetitions: count,
            measurement_status: "measured".to_owned(),
            requests_per_turn: 1,
            requests_per_minute_limit: DOCUMENTED_REQUESTS_PER_MINUTE,
            target_turns_per_minute: 3,
            maximum_sustainable_turns_per_minute_milli: 600_000,
            rate_limit_feasible: true,
            infeasibility_basis: None,
            planned_worst_case_requests: count as u64,
            planned_nominal_requests: count as u64,
            estimated_input_tokens_per_turn: 0,
            estimated_max_input_tokens_per_request: 0,
            estimated_max_state_and_question_tokens: 0,
            heuristic_token_limit_risk: false,
            estimated_total_cost_nano_usd: None,
        };
        let samples = (0..count)
            .map(|repetition| BenchSample {
                repetition,
                invocation_id: format!("i-{repetition}"),
                resolved_model_ids: Vec::new(),
                end_to_end_latency_micros: latency_micros,
                attempt_latency_micros: vec![latency_micros],
                attempt_count: 1,
                successful_provider_responses: 0,
                max_input_tokens_per_request: None,
                retry_reasons: Vec::new(),
                input_tokens: None,
                output_tokens: None,
                failure: None,
            })
            .collect::<Vec<_>>();
        aggregate_cell(plan, samples, Duration::ZERO, 0, 10, None)
    }

    async fn stub_response(
        State(counter): State<Arc<AtomicUsize>>,
        Json(request): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        counter.fetch_add(1, Ordering::SeqCst);
        let questions = request["questions"].as_object().expect("questions");
        let answers = questions
            .keys()
            .map(|id| (id.clone(), json!({"type":"noul","noul":0.75})))
            .collect::<serde_json::Map<_, _>>();
        Json(json!({
            "model":"jev-1.13.0-resolved",
            "answers":answers,
            "usage":{"input_tokens":120,"output_tokens":8}
        }))
    }
}
