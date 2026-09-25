//! Hosted System One HTTP implementation of pair scoring.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use engine_logging::engine_error;
use futures_util::{StreamExt, stream};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    budgets::validate_timing_budget,
    pair_scoring::{
        InjectionDeadline, PairScore, PairScoreRequest, PairScoringService, QuestionShaping,
        ScoreKind,
    },
    trace::{
        BackendKind, ModelIdentity, ModelIdentityKind, ParsedUsage, SemanticFailure,
        SemanticOperation, SemanticTraceRecord, ServiceTracePayload, Traced, invocation_prefix,
    },
};

/// Current wording for the hosted `noul` question. Threshold tuning owns later changes.
pub const QUESTION_WORDING_VERSION: &str = "relevance_noul_v1";

const QUESTION_INSTRUCTIONS: &str = "Decide whether the candidate text is about the utterance. Answer yes only when the candidate is substantively relevant to what the utterance says, asks, or means.";
const QUESTION_TRUE_CRITERIA: &str = "The candidate is substantively about the utterance.";
const QUESTION_FALSE_CRITERIA: &str =
    "The candidate is unrelated or only shares incidental wording.";

/// Configuration for a hosted System One judge.
#[derive(Clone, Debug)]
pub struct RemoteRelevanceJudgeConfig {
    /// Service base URL, normally `https://api.typesafe.ai`.
    pub base_url: String,
    /// Bearer token for the hosted service.
    pub api_key: String,
    /// Explicit pinned version requested from the service.
    pub pinned_model_id: String,
    /// Maximum attempts including the first request.
    pub max_attempts: u32,
    /// Initial retry delay.
    pub initial_backoff: Duration,
    /// Upper bound for exponential retry delay.
    pub max_backoff: Duration,
    /// Individual request ceiling, distinct from injection waiting.
    pub request_timeout: Duration,
    /// Maximum in-flight hosted requests.
    pub max_concurrency: usize,
}

impl RemoteRelevanceJudgeConfig {
    fn validate_static(&self) -> Result<(), SemanticFailure> {
        if self.base_url.trim().is_empty()
            || self.api_key.trim().is_empty()
            || self.pinned_model_id.trim().is_empty()
        {
            return Err(SemanticFailure::BackendUnavailable {
                detail: "remote_http requires base URL, API key, and pinned model id".to_owned(),
            });
        }
        if self.max_attempts == 0 || self.max_concurrency == 0 {
            return Err(SemanticFailure::BackendUnavailable {
                detail: "remote_http max_attempts and max_concurrency must be positive".to_owned(),
            });
        }
        Ok(())
    }

    /// Checks required fields and the timer relationship before any call is attempted.
    pub fn validate(&self, injection_deadline: InjectionDeadline) -> Result<(), SemanticFailure> {
        self.validate_static()?;
        validate_timing_budget(injection_deadline.duration(), self.request_timeout).map_err(
            |error| SemanticFailure::BackendUnavailable {
                detail: error.to_string(),
            },
        )?;
        Ok(())
    }
}

/// Hosted HTTP judge with bounded retries and concurrency.
#[derive(Clone)]
pub struct RemoteRelevanceJudge {
    config: RemoteRelevanceJudgeConfig,
    client: Client,
    permits: Arc<Semaphore>,
    invocation_prefix: String,
    invocation_sequence: Arc<AtomicU64>,
    request_budget: Option<(Arc<AtomicU64>, u64)>,
}

impl RemoteRelevanceJudge {
    /// Builds a judge from explicit configuration.
    pub fn new(config: RemoteRelevanceJudgeConfig) -> Result<Self, SemanticFailure> {
        config.validate_static()?;
        Ok(Self {
            permits: Arc::new(Semaphore::new(config.max_concurrency)),
            config,
            client: Client::new(),
            invocation_prefix: invocation_prefix("remote-http"),
            invocation_sequence: Arc::new(AtomicU64::new(1)),
            request_budget: None,
        })
    }

    /// Shares a hard physical-request ceiling with the offline bench harness.
    pub fn with_request_budget(mut self, maximum: u64) -> (Self, Arc<AtomicU64>) {
        let sent = Arc::new(AtomicU64::new(0));
        self.request_budget = Some((sent.clone(), maximum));
        (self, sent)
    }

    fn invocation_id(&self) -> String {
        format!(
            "{}-{}",
            self.invocation_prefix,
            self.invocation_sequence.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/v1/systemone",
            self.config.base_url.trim_end_matches('/')
        )
    }

    async fn invoke(
        &self,
        body: SystemOneRequest,
        question_ids: Vec<String>,
        invocation_id: &str,
    ) -> Result<RemoteCall, AttemptFailure> {
        let mut retry_reasons = Vec::new();
        let mut attempt_latency_micros = Vec::new();
        for attempt in 1..=self.config.max_attempts {
            let permit = self.permits.clone().acquire_owned().await.map_err(|_| {
                AttemptFailure::terminal(SemanticFailure::BackendUnavailable {
                    detail: "remote_http concurrency limiter closed".to_owned(),
                })
            })?;
            if let Some((sent, maximum)) = &self.request_budget {
                if sent
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                        (current < *maximum).then_some(current + 1)
                    })
                    .is_err()
                {
                    return Err(AttemptFailure {
                        failure: SemanticFailure::BackendUnavailable {
                            detail: "bench_request_cap_reached".to_owned(),
                        },
                        attempts: attempt - 1,
                        retry_reasons,
                        attempt_latency_micros,
                        partial_successes: Vec::new(),
                    });
                }
            }
            let attempt_started = Instant::now();
            let endpoint = self.endpoint();
            let request = self
                .client
                .post(&endpoint)
                .bearer_auth(&self.config.api_key)
                .json(&body)
                .send();
            let result = tokio::time::timeout(self.config.request_timeout, request).await;
            match result {
                Err(_) => {
                    attempt_latency_micros.push(attempt_started.elapsed().as_micros() as u64);
                    engine_error!(
                        "semantic remote request failed invocation_id={} backend=remote_http endpoint={} attempt={} status=timeout",
                        invocation_id,
                        endpoint,
                        attempt
                    );
                    return Err(AttemptFailure {
                        failure: SemanticFailure::Timeout,
                        attempts: attempt,
                        retry_reasons,
                        attempt_latency_micros,
                        partial_successes: Vec::new(),
                    });
                }
                Ok(Err(error)) => {
                    attempt_latency_micros.push(attempt_started.elapsed().as_micros() as u64);
                    engine_error!(
                        "semantic remote request failed invocation_id={} backend=remote_http endpoint={} attempt={} status=transport",
                        invocation_id,
                        endpoint,
                        attempt
                    );
                    return Err(AttemptFailure {
                        failure: SemanticFailure::Transport {
                            detail: error.to_string(),
                        },
                        attempts: attempt,
                        retry_reasons,
                        attempt_latency_micros,
                        partial_successes: Vec::new(),
                    });
                }
                Ok(Ok(response)) => {
                    let status = response.status();
                    if !status.is_success() {
                        attempt_latency_micros.push(attempt_started.elapsed().as_micros() as u64);
                        drop(permit);
                        let failure = failure_for_status(status);
                        engine_error!(
                            "semantic remote request failed invocation_id={} backend=remote_http endpoint={} attempt={} status={}",
                            invocation_id,
                            endpoint,
                            attempt,
                            status.as_u16()
                        );
                        if is_retryable(&failure) && attempt < self.config.max_attempts {
                            retry_reasons.push(failure_name(&failure).to_owned());
                            tokio::time::sleep(backoff_delay(
                                self.config.initial_backoff,
                                self.config.max_backoff,
                                attempt,
                            ))
                            .await;
                            continue;
                        }
                        return Err(AttemptFailure {
                            failure,
                            attempts: attempt,
                            retry_reasons,
                            attempt_latency_micros,
                            partial_successes: Vec::new(),
                        });
                    }
                    let response_body = tokio::time::timeout(
                        self.config.request_timeout,
                        response.json::<SystemOneResponse>(),
                    )
                    .await;
                    attempt_latency_micros.push(attempt_started.elapsed().as_micros() as u64);
                    drop(permit);
                    let response_body = match response_body {
                        Err(_) => {
                            engine_error!(
                                "semantic remote request failed invocation_id={} backend=remote_http endpoint={} attempt={} status=timeout_decode",
                                invocation_id,
                                endpoint,
                                attempt
                            );
                            return Err(AttemptFailure {
                                failure: SemanticFailure::Timeout,
                                attempts: attempt,
                                retry_reasons,
                                attempt_latency_micros,
                                partial_successes: Vec::new(),
                            });
                        }
                        Ok(Err(error)) => {
                            engine_error!(
                                "semantic remote request failed invocation_id={} backend=remote_http endpoint={} attempt={} status=decode",
                                invocation_id,
                                endpoint,
                                attempt
                            );
                            return Err(AttemptFailure {
                                failure: SemanticFailure::Decode {
                                    detail: error.to_string(),
                                },
                                attempts: attempt,
                                retry_reasons,
                                attempt_latency_micros,
                                partial_successes: Vec::new(),
                            });
                        }
                        Ok(Ok(body)) => body,
                    };
                    let mut probabilities = BTreeMap::new();
                    for question_id in question_ids {
                        let answer = response_body.answers.get(&question_id).ok_or_else(|| {
                            AttemptFailure {
                                failure: SemanticFailure::Decode {
                                    detail: format!(
                                        "response omitted answer for question {question_id}"
                                    ),
                                },
                                attempts: attempt,
                                retry_reasons: retry_reasons.clone(),
                                attempt_latency_micros: attempt_latency_micros.clone(),
                                partial_successes: Vec::new(),
                            }
                        })?;
                        if answer.answer_type != "noul" {
                            return Err(AttemptFailure {
                                failure: SemanticFailure::Decode {
                                    detail: format!("answer {question_id} was not noul"),
                                },
                                attempts: attempt,
                                retry_reasons,
                                attempt_latency_micros,
                                partial_successes: Vec::new(),
                            });
                        }
                        probabilities.insert(
                            question_id,
                            probability_to_basis_points(answer.noul).map_err(|failure| {
                                AttemptFailure {
                                    failure,
                                    attempts: attempt,
                                    retry_reasons: retry_reasons.clone(),
                                    attempt_latency_micros: attempt_latency_micros.clone(),
                                    partial_successes: Vec::new(),
                                }
                            })?,
                        );
                    }
                    let usage_raw = response_body.usage.clone();
                    let usage_parsed = parse_usage(&usage_raw).map_err(|mut failure| {
                        failure.attempts = attempt;
                        failure.retry_reasons = retry_reasons.clone();
                        failure.attempt_latency_micros = attempt_latency_micros.clone();
                        failure
                    })?;
                    return Ok(RemoteCall {
                        probabilities,
                        model: response_body.model,
                        usage_raw,
                        usage_parsed,
                        attempts: attempt,
                        retry_reasons,
                        attempt_latency_micros,
                    });
                }
            }
        }
        Err(AttemptFailure::terminal(
            SemanticFailure::BackendUnavailable {
                detail: "remote retry loop exhausted unexpectedly".to_owned(),
            },
        ))
    }

    async fn score_remote(
        &self,
        request: PairScoreRequest,
        injection_deadline: InjectionDeadline,
    ) -> Traced<Vec<PairScore>> {
        let invocation_id = self.invocation_id();
        let started = Instant::now();
        let injection_deadline_ms = injection_deadline.duration().as_millis() as u64;
        if let Err(failure) = self.config.validate(injection_deadline) {
            return Traced::failure_with_trace(
                self.trace(
                    &request,
                    &invocation_id,
                    injection_deadline_ms,
                    TraceCompletion::failure(failure.clone()),
                ),
                failure,
            );
        }
        if request.options.question_wording_version != QUESTION_WORDING_VERSION {
            let failure = SemanticFailure::InvalidRequest {
                detail: format!(
                    "unsupported question wording version {}",
                    request.options.question_wording_version
                ),
            };
            return Traced::failure_with_trace(
                self.trace(
                    &request,
                    &invocation_id,
                    injection_deadline_ms,
                    TraceCompletion::failure(failure.clone()),
                ),
                failure,
            );
        }
        if request.options.score_kind_expected != ScoreKind::Probability {
            let failure = SemanticFailure::InvalidRequest {
                detail: "remote_http only provides probability scores".to_owned(),
            };
            return Traced::failure_with_trace(
                self.trace(
                    &request,
                    &invocation_id,
                    injection_deadline_ms,
                    TraceCompletion::failure(failure.clone()),
                ),
                failure,
            );
        }
        if duplicate_candidate_ids(&request) {
            let failure = SemanticFailure::InvalidRequest {
                detail: "candidate ids must be unique question ids".to_owned(),
            };
            return Traced::failure_with_trace(
                self.trace(
                    &request,
                    &invocation_id,
                    injection_deadline_ms,
                    TraceCompletion::failure(failure.clone()),
                ),
                failure,
            );
        }

        if request.candidates.is_empty() {
            let trace = self.trace(
                &request,
                &invocation_id,
                injection_deadline_ms,
                TraceCompletion::success_without_call(started.elapsed().as_micros() as u64),
            );
            return Traced::success(trace, Vec::new());
        }

        let calls = match request.options.shaping {
            QuestionShaping::SharedStateQuestions => {
                let ids = request
                    .candidates
                    .iter()
                    .map(|candidate| candidate.candidate_id.clone())
                    .collect::<Vec<_>>();
                let mut body = shared_request(&request);
                body.model = self.config.pinned_model_id.clone();
                match self.invoke(body, ids, &invocation_id).await {
                    Ok(call) => Ok(vec![call]),
                    Err(error) => Err(error),
                }
            }
            QuestionShaping::PerCandidateRequest => {
                let endpoint_calls = request.candidates.iter().cloned().map(|candidate| {
                    let judge = self.clone();
                    let mut call_request = per_candidate_request(&request, &candidate);
                    call_request.model = judge.config.pinned_model_id.clone();
                    let id = invocation_id.clone();
                    async move {
                        judge
                            .invoke(call_request, vec![candidate.candidate_id], &id)
                            .await
                    }
                });
                let results = stream::iter(endpoint_calls)
                    .buffer_unordered(self.config.max_concurrency)
                    .collect::<Vec<_>>()
                    .await;
                let mut calls = Vec::new();
                let mut failure: Option<AttemptFailure> = None;
                for result in results {
                    match result {
                        Ok(call) => calls.push(call),
                        Err(error) => {
                            if let Some(aggregate) = &mut failure {
                                aggregate.attempts += error.attempts;
                                aggregate
                                    .attempt_latency_micros
                                    .extend(error.attempt_latency_micros);
                                aggregate.retry_reasons.extend(error.retry_reasons);
                                aggregate.partial_successes.extend(error.partial_successes);
                            } else {
                                failure = Some(error);
                            }
                        }
                    }
                }
                if let Some(mut failure) = failure {
                    for call in &calls {
                        failure.attempts += call.attempts;
                        failure
                            .attempt_latency_micros
                            .extend(call.attempt_latency_micros.iter().copied());
                        failure
                            .retry_reasons
                            .extend(call.retry_reasons.iter().cloned());
                    }
                    failure.partial_successes = calls;
                    Err(failure)
                } else {
                    Ok(calls)
                }
            }
        };
        let latency_micros = started.elapsed().as_micros() as u64;
        match calls {
            Ok(calls) => {
                let mut probabilities = BTreeMap::new();
                let mut usage_values = Vec::new();
                let mut input_tokens = 0;
                let mut output_tokens = 0;
                let mut attempts = 0;
                let mut attempt_latencies = Vec::new();
                let mut retry_reasons = Vec::new();
                let mut resolved_models = BTreeSet::new();
                for call in calls {
                    probabilities.extend(call.probabilities);
                    usage_values.push(call.usage_raw);
                    input_tokens += call.usage_parsed.input_tokens;
                    output_tokens += call.usage_parsed.output_tokens;
                    attempts += call.attempts;
                    attempt_latencies.extend(call.attempt_latency_micros);
                    retry_reasons.extend(call.retry_reasons);
                    resolved_models.insert(call.model);
                }
                let scores = request
                    .candidates
                    .iter()
                    .map(|candidate| {
                        probabilities
                            .get(&candidate.candidate_id)
                            .copied()
                            .map(|score_basis_points| PairScore {
                                candidate_id: candidate.candidate_id.clone(),
                                candidate_content_hash: candidate.candidate_content_hash.clone(),
                                score_basis_points,
                                score_kind: ScoreKind::Probability,
                                abstained: false,
                                abstain_reason: None,
                            })
                            .ok_or_else(|| SemanticFailure::Decode {
                                detail: format!(
                                    "aggregated response omitted candidate {}",
                                    candidate.candidate_id
                                ),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>();
                let scores = match scores {
                    Ok(scores) => scores,
                    Err(failure) => {
                        let trace = self.trace(
                            &request,
                            &invocation_id,
                            injection_deadline_ms,
                            TraceCompletion {
                                latency_micros,
                                response_models: resolved_models.into_iter().collect(),
                                usage: None,
                                attempt_count: attempts,
                                attempt_latency_micros: attempt_latencies,
                                retry_reasons,
                                failure: Some(failure.clone()),
                            },
                        );
                        return Traced::failure_with_trace(trace, failure);
                    }
                };
                let usage_raw = match usage_values.len() {
                    0 => None,
                    1 => usage_values.into_iter().next(),
                    _ => Some(Value::Array(usage_values)),
                };
                let usage_parsed = ParsedUsage {
                    input_tokens,
                    output_tokens,
                };
                let trace = self.trace(
                    &request,
                    &invocation_id,
                    injection_deadline_ms,
                    TraceCompletion {
                        latency_micros,
                        response_models: resolved_models.into_iter().collect(),
                        usage: Some((usage_raw, usage_parsed)),
                        attempt_count: attempts,
                        attempt_latency_micros: attempt_latencies,
                        retry_reasons,
                        failure: None,
                    },
                );
                Traced::success(trace, scores)
            }
            Err(error) => {
                let mut response_models = BTreeSet::new();
                let mut usage_values = Vec::new();
                let mut input_tokens = 0;
                let mut output_tokens = 0;
                for call in &error.partial_successes {
                    response_models.insert(call.model.clone());
                    usage_values.push(call.usage_raw.clone());
                    input_tokens += call.usage_parsed.input_tokens;
                    output_tokens += call.usage_parsed.output_tokens;
                }
                let usage_raw = match usage_values.len() {
                    0 => None,
                    1 => usage_values.into_iter().next(),
                    _ => Some(Value::Array(usage_values)),
                };
                let usage = (!error.partial_successes.is_empty()).then_some({
                    (
                        usage_raw,
                        ParsedUsage {
                            input_tokens,
                            output_tokens,
                        },
                    )
                });
                let failure = error.failure.clone();
                let trace = self.trace(
                    &request,
                    &invocation_id,
                    injection_deadline_ms,
                    TraceCompletion {
                        latency_micros,
                        response_models: response_models.into_iter().collect(),
                        usage,
                        attempt_count: error.attempts,
                        attempt_latency_micros: error.attempt_latency_micros,
                        retry_reasons: error.retry_reasons,
                        failure: Some(failure.clone()),
                    },
                );
                Traced::failure_with_trace(trace, failure)
            }
        }
    }

    fn trace(
        &self,
        request: &PairScoreRequest,
        invocation_id: &str,
        injection_deadline_ms: u64,
        completion: TraceCompletion,
    ) -> SemanticTraceRecord {
        let resolved_model = completion
            .response_models
            .first()
            .cloned()
            .unwrap_or_else(|| self.config.pinned_model_id.clone());
        SemanticTraceRecord {
            invocation_id: invocation_id.to_owned(),
            task: request.task,
            operation: SemanticOperation::PairScore,
            backend_kind: BackendKind::RemoteHttp,
            model_identity: ModelIdentity {
                backend: BackendKind::RemoteHttp,
                model_id: resolved_model.clone(),
                identity_kind: ModelIdentityKind::PinnedRemoteVersion,
                identity_value: resolved_model,
            },
            shaping: request.options.shaping,
            question_wording_version: request.options.question_wording_version.clone(),
            score_kind: ScoreKind::Probability,
            injection_deadline_ms,
            request_timeout_ms: Some(self.config.request_timeout.as_millis() as u64),
            latency_micros: completion.latency_micros,
            failure: completion.failure,
            service: Some(ServiceTracePayload {
                attempt_count: completion.attempt_count,
                attempt_latency_micros: completion.attempt_latency_micros,
                retry_reasons: completion.retry_reasons,
                usage_raw: completion.usage.as_ref().and_then(|(raw, _)| raw.clone()),
                usage_parsed: completion.usage.map(|(_, parsed)| parsed),
                requested_model_id: self.config.pinned_model_id.clone(),
                resolved_model_ids: completion.response_models,
            }),
            local_encoder: None,
        }
    }
}

impl PairScoringService for RemoteRelevanceJudge {
    fn score_pairs(
        &self,
        request: PairScoreRequest,
        injection_deadline: InjectionDeadline,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Traced<Vec<PairScore>>> + Send + '_>>
    {
        Box::pin(async move { self.score_remote(request, injection_deadline).await })
    }
}

/// Maps documented HTTP statuses to the service's typed failure variants.
pub fn failure_for_status(status: StatusCode) -> SemanticFailure {
    match status.as_u16() {
        401 => SemanticFailure::Unauthorized,
        422 => SemanticFailure::InvalidRequest {
            detail: "HTTP 422".to_owned(),
        },
        429 => SemanticFailure::RateLimited,
        529 => SemanticFailure::Overloaded,
        code => SemanticFailure::Transport {
            detail: format!("unexpected HTTP status {code}"),
        },
    }
}

/// Whether a failure is retried with bounded exponential backoff.
pub fn is_retryable(failure: &SemanticFailure) -> bool {
    matches!(
        failure,
        SemanticFailure::RateLimited | SemanticFailure::Overloaded
    )
}

/// Computes a capped exponential delay; attempt one uses the initial delay.
pub fn backoff_delay(initial: Duration, maximum: Duration, attempt: u32) -> Duration {
    let multiplier = 1_u32
        .checked_shl(attempt.saturating_sub(1))
        .unwrap_or(u32::MAX);
    initial.saturating_mul(multiplier).min(maximum)
}

/// Converts a provider `noul` probability to basis points by rounding to nearest integer and clamping to `0..=10000`.
pub fn probability_to_basis_points(probability: f64) -> Result<u16, SemanticFailure> {
    if !probability.is_finite() {
        return Err(SemanticFailure::Decode {
            detail: "provider noul probability was not finite".to_owned(),
        });
    }
    Ok((probability * 10_000.0).round().clamp(0.0, 10_000.0) as u16)
}

fn failure_name(failure: &SemanticFailure) -> &'static str {
    match failure {
        SemanticFailure::RateLimited => "rate_limited",
        SemanticFailure::Overloaded => "overloaded",
        _ => "other",
    }
}

fn duplicate_candidate_ids(request: &PairScoreRequest) -> bool {
    request
        .candidates
        .iter()
        .map(|candidate| &candidate.candidate_id)
        .collect::<BTreeSet<_>>()
        .len()
        != request.candidates.len()
}

fn parse_usage(raw: &Value) -> Result<ParsedUsage, AttemptFailure> {
    let usage = serde_json::from_value::<Usage>(raw.clone()).map_err(|error| {
        AttemptFailure::terminal(SemanticFailure::Decode {
            detail: format!("usage: {error}"),
        })
    })?;
    Ok(ParsedUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
    })
}

fn shared_request(request: &PairScoreRequest) -> SystemOneRequest {
    let state = json!({
        "utterance": request.utterance_text,
        "candidates": request.candidates.iter().map(|candidate| json!({"candidate_id": candidate.candidate_id, "candidate_text": candidate.candidate_text})).collect::<Vec<_>>(),
    });
    SystemOneRequest {
        state,
        model: "".to_owned(),
        questions: request
            .candidates
            .iter()
            .map(|candidate| (candidate.candidate_id.clone(), noul_question()))
            .collect(),
    }
}

fn per_candidate_request(
    request: &PairScoreRequest,
    candidate: &crate::pair_scoring::Candidate,
) -> SystemOneRequest {
    SystemOneRequest {
        state: json!({ "utterance": request.utterance_text, "candidate": { "candidate_id": candidate.candidate_id, "candidate_text": candidate.candidate_text } }),
        model: "".to_owned(),
        questions: BTreeMap::from([(candidate.candidate_id.clone(), noul_question())]),
    }
}

/// Serialized body and state-plus-longest-question byte sizes for bench heuristics.
pub(crate) fn request_body_sizes(
    request: &PairScoreRequest,
    model_id: &str,
) -> Vec<(usize, usize)> {
    let bodies = match request.options.shaping {
        QuestionShaping::SharedStateQuestions => vec![shared_request(request)],
        QuestionShaping::PerCandidateRequest => request
            .candidates
            .iter()
            .map(|candidate| per_candidate_request(request, candidate))
            .collect(),
    };
    bodies
        .into_iter()
        .map(|mut body| {
            body.model = model_id.to_owned();
            let full = serde_json::to_vec(&body)
                .expect("request body is serializable")
                .len();
            let state = serde_json::to_vec(&body.state)
                .expect("state is serializable")
                .len();
            let longest_question = body
                .questions
                .iter()
                .map(|(id, question)| {
                    serde_json::to_vec(&(id, question))
                        .expect("question is serializable")
                        .len()
                })
                .max()
                .unwrap_or_default();
            (full, state + longest_question)
        })
        .collect()
}

fn noul_question() -> SystemOneQuestion {
    SystemOneQuestion {
        question_type: "noul".to_owned(),
        instructions: QUESTION_INSTRUCTIONS.to_owned(),
        criteria: json!({ "true": QUESTION_TRUE_CRITERIA, "false": QUESTION_FALSE_CRITERIA }),
    }
}

#[derive(Clone, Debug, Serialize)]
struct SystemOneRequest {
    state: Value,
    model: String,
    questions: BTreeMap<String, SystemOneQuestion>,
}

#[derive(Clone, Debug, Serialize)]
struct SystemOneQuestion {
    #[serde(rename = "type")]
    question_type: String,
    instructions: String,
    criteria: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemOneResponse {
    model: String,
    answers: BTreeMap<String, SystemOneAnswer>,
    usage: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemOneAnswer {
    #[serde(rename = "type")]
    answer_type: String,
    noul: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug)]
struct RemoteCall {
    probabilities: BTreeMap<String, u16>,
    model: String,
    usage_raw: Value,
    usage_parsed: ParsedUsage,
    attempts: u32,
    attempt_latency_micros: Vec<u64>,
    retry_reasons: Vec<String>,
}

#[derive(Debug)]
struct AttemptFailure {
    failure: SemanticFailure,
    attempts: u32,
    attempt_latency_micros: Vec<u64>,
    retry_reasons: Vec<String>,
    partial_successes: Vec<RemoteCall>,
}

struct TraceCompletion {
    latency_micros: u64,
    response_models: Vec<String>,
    usage: Option<(Option<Value>, ParsedUsage)>,
    attempt_count: u32,
    attempt_latency_micros: Vec<u64>,
    retry_reasons: Vec<String>,
    failure: Option<SemanticFailure>,
}

impl TraceCompletion {
    fn failure(failure: SemanticFailure) -> Self {
        Self {
            latency_micros: 0,
            response_models: Vec::new(),
            usage: None,
            attempt_count: 0,
            attempt_latency_micros: Vec::new(),
            retry_reasons: Vec::new(),
            failure: Some(failure),
        }
    }

    fn success_without_call(latency_micros: u64) -> Self {
        Self {
            latency_micros,
            response_models: Vec::new(),
            usage: None,
            attempt_count: 0,
            attempt_latency_micros: Vec::new(),
            retry_reasons: Vec::new(),
            failure: None,
        }
    }
}

impl AttemptFailure {
    fn terminal(failure: SemanticFailure) -> Self {
        Self {
            failure,
            attempts: 1,
            attempt_latency_micros: Vec::new(),
            retry_reasons: Vec::new(),
            partial_successes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        fs,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use axum::{Json, Router, extract::State, routing::post};

    use crate::{
        pair_scoring::{Candidate, CandidateKind, PairScoringOptions, RelevanceTask},
        trace::{
            CompletionDisposition, InvocationIdentity, RelevanceJudgmentCompletedRecord,
            SemanticLifecycleRecord, completed_candidates,
        },
    };

    use super::*;

    #[derive(Clone)]
    struct StubState {
        statuses: Arc<Mutex<VecDeque<StatusCode>>>,
        models: Arc<Mutex<VecDeque<String>>>,
        delay: Duration,
        request_count: Arc<AtomicUsize>,
        in_flight: Arc<AtomicUsize>,
        max_in_flight: Arc<AtomicUsize>,
    }

    struct StubServer {
        endpoint: String,
        state: StubState,
    }

    fn request(shaping: QuestionShaping, candidate_count: usize) -> PairScoreRequest {
        PairScoreRequest {
            task: RelevanceTask::MemoryRelevance,
            utterance_text: "I need help with my garden".to_owned(),
            utterance_content_hash: "utterance-hash".to_owned(),
            candidates: (0..candidate_count)
                .map(|index| Candidate {
                    candidate_id: format!("candidate-{index}"),
                    candidate_text: format!("Garden fact {index}"),
                    candidate_content_hash: format!("candidate-hash-{index}"),
                    candidate_kind: CandidateKind("memory".to_owned()),
                })
                .collect(),
            options: PairScoringOptions {
                question_wording_version: QUESTION_WORDING_VERSION.to_owned(),
                shaping,
                score_kind_expected: ScoreKind::Probability,
            },
        }
    }

    fn remote_judge(
        endpoint: String,
        request_timeout: Duration,
        max_attempts: u32,
        max_concurrency: usize,
    ) -> RemoteRelevanceJudge {
        RemoteRelevanceJudge::new(RemoteRelevanceJudgeConfig {
            base_url: endpoint,
            api_key: "test-key".to_owned(),
            pinned_model_id: "jev-1.13.0".to_owned(),
            max_attempts,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(2),
            request_timeout,
            max_concurrency,
        })
        .expect("remote judge")
    }

    async fn start_stub(
        statuses: Vec<StatusCode>,
        models: Vec<&str>,
        delay: Duration,
    ) -> StubServer {
        let state = StubState {
            statuses: Arc::new(Mutex::new(statuses.into())),
            models: Arc::new(Mutex::new(models.into_iter().map(str::to_owned).collect())),
            delay,
            request_count: Arc::new(AtomicUsize::new(0)),
            in_flight: Arc::new(AtomicUsize::new(0)),
            max_in_flight: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route("/v1/systemone", post(stub_response))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let address = listener.local_addr().expect("stub address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve stub") });
        StubServer {
            endpoint: format!("http://{address}"),
            state,
        }
    }

    async fn stub_response(
        State(state): State<StubState>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        state.request_count.fetch_add(1, Ordering::SeqCst);
        let current = state.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        state.max_in_flight.fetch_max(current, Ordering::SeqCst);
        let status = state
            .statuses
            .lock()
            .expect("statuses")
            .pop_front()
            .unwrap_or(StatusCode::OK);
        if !state.delay.is_zero() {
            tokio::time::sleep(state.delay).await;
        }
        state.in_flight.fetch_sub(1, Ordering::SeqCst);
        if status != StatusCode::OK {
            return (status, Json(json!({"error": "stub"})));
        }
        let model = state
            .models
            .lock()
            .expect("models")
            .pop_front()
            .unwrap_or_else(|| "jev-1.13.0-resolved".to_owned());
        let answers = body["questions"]
            .as_object()
            .expect("questions")
            .keys()
            .map(|id| (id.clone(), json!({"type":"noul", "noul":0.9325})))
            .collect::<serde_json::Map<_, _>>();
        (
            StatusCode::OK,
            Json(
                json!({"model":model, "answers":answers, "usage":{"input_tokens":12,"output_tokens":3}}),
            ),
        )
    }

    #[test]
    fn probability_conversion_rounds_clamps_and_rejects_non_finite_values() {
        assert_eq!(probability_to_basis_points(0.0), Ok(0));
        assert_eq!(probability_to_basis_points(1.0), Ok(10_000));
        assert_eq!(probability_to_basis_points(0.123_44), Ok(1_234));
        assert_eq!(probability_to_basis_points(0.123_45), Ok(1_235));
        assert_eq!(probability_to_basis_points(-1.0), Ok(0));
        assert_eq!(probability_to_basis_points(2.0), Ok(10_000));
        assert!(matches!(
            probability_to_basis_points(f64::NAN),
            Err(SemanticFailure::Decode { .. })
        ));
        assert!(matches!(
            probability_to_basis_points(f64::INFINITY),
            Err(SemanticFailure::Decode { .. })
        ));
    }

    #[test]
    fn retry_backoff_is_exponential_and_bounded() {
        assert_eq!(
            backoff_delay(Duration::from_millis(10), Duration::from_millis(35), 1),
            Duration::from_millis(10)
        );
        assert_eq!(
            backoff_delay(Duration::from_millis(10), Duration::from_millis(35), 2),
            Duration::from_millis(20)
        );
        assert_eq!(
            backoff_delay(Duration::from_millis(10), Duration::from_millis(35), 3),
            Duration::from_millis(35)
        );
    }

    #[tokio::test]
    async fn documented_http_statuses_map_to_typed_failures() {
        for (status, expected) in [
            (StatusCode::UNAUTHORIZED, SemanticFailure::Unauthorized),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                SemanticFailure::InvalidRequest {
                    detail: "HTTP 422".to_owned(),
                },
            ),
            (StatusCode::TOO_MANY_REQUESTS, SemanticFailure::RateLimited),
            (
                StatusCode::from_u16(529).expect("status"),
                SemanticFailure::Overloaded,
            ),
        ] {
            let stub = start_stub(vec![status], vec![], Duration::ZERO).await;
            let judge = remote_judge(stub.endpoint, Duration::from_millis(100), 1, 2);
            let traced = judge
                .score_pairs(
                    request(QuestionShaping::SharedStateQuestions, 1),
                    InjectionDeadline::from_duration(Duration::from_millis(5)),
                )
                .await;
            assert_eq!(traced.outcome.expect_err("failure"), expected);
            assert_eq!(traced.trace.failure, Some(expected));
        }
    }

    #[tokio::test]
    async fn retryable_status_then_success_records_attempt_and_reason() {
        let stub = start_stub(
            vec![StatusCode::TOO_MANY_REQUESTS, StatusCode::OK],
            vec!["jev-1.13.0-resolved"],
            Duration::ZERO,
        )
        .await;
        let judge = remote_judge(stub.endpoint, Duration::from_millis(100), 2, 2);
        let traced = judge
            .score_pairs(
                request(QuestionShaping::SharedStateQuestions, 1),
                InjectionDeadline::from_duration(Duration::from_millis(5)),
            )
            .await;
        assert!(traced.outcome.is_ok());
        let service = traced.trace.service.expect("service trace");
        assert_eq!(service.attempt_count, 2);
        assert_eq!(
            service.attempt_latency_micros.len(),
            service.attempt_count as usize
        );
        assert_eq!(service.retry_reasons, ["rate_limited"]);
    }

    #[tokio::test]
    async fn injection_deadline_expiry_does_not_cancel_and_preserves_model_discrepancy() {
        let stub = start_stub(
            vec![StatusCode::OK],
            vec!["jev-1.13.0-resolved"],
            Duration::from_millis(70),
        )
        .await;
        let judge = remote_judge(stub.endpoint, Duration::from_millis(150), 1, 2);
        let deadline = InjectionDeadline::from_duration(Duration::from_millis(10));
        let mut completion = tokio::spawn(async move {
            judge
                .score_pairs(request(QuestionShaping::SharedStateQuestions, 1), deadline)
                .await
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut completion)
                .await
                .is_err()
        );
        let completed = completion.await.expect("request task joins");
        assert!(completed.outcome.is_ok());
        assert_eq!(completed.trace.injection_deadline_ms, 10);
        assert_eq!(
            completed.trace.model_identity.identity_value,
            "jev-1.13.0-resolved"
        );
        let service = completed.trace.service.expect("service trace");
        assert_eq!(service.requested_model_id, "jev-1.13.0");
        assert_eq!(service.resolved_model_ids, ["jev-1.13.0-resolved"]);
    }

    #[tokio::test]
    async fn per_candidate_shaping_bounds_calls_and_aggregates_usage_and_models() {
        let stub = start_stub(
            vec![StatusCode::OK; 3],
            vec!["jev-a", "jev-b", "jev-a"],
            Duration::from_millis(30),
        )
        .await;
        let judge = remote_judge(stub.endpoint.clone(), Duration::from_millis(200), 1, 2);
        let traced = judge
            .score_pairs(
                request(QuestionShaping::PerCandidateRequest, 3),
                InjectionDeadline::from_duration(Duration::from_millis(10)),
            )
            .await;
        assert_eq!(traced.outcome.expect("scores").len(), 3);
        assert_eq!(stub.state.request_count.load(Ordering::SeqCst), 3);
        assert_eq!(stub.state.max_in_flight.load(Ordering::SeqCst), 2);
        assert_eq!(traced.trace.shaping, QuestionShaping::PerCandidateRequest);
        let service = traced.trace.service.expect("service trace");
        assert!(matches!(service.usage_raw, Some(Value::Array(ref values)) if values.len() == 3));
        assert_eq!(
            service.usage_parsed,
            Some(ParsedUsage {
                input_tokens: 36,
                output_tokens: 9,
            })
        );
        assert_eq!(service.resolved_model_ids, ["jev-a", "jev-b"]);
    }

    #[tokio::test]
    async fn per_candidate_failure_preserves_partial_usage_and_attempts() {
        let stub = start_stub(
            vec![StatusCode::OK, StatusCode::UNAUTHORIZED, StatusCode::OK],
            vec!["jev-a", "jev-b"],
            Duration::ZERO,
        )
        .await;
        let judge = remote_judge(stub.endpoint, Duration::from_millis(100), 1, 1);
        let traced = judge
            .score_pairs(
                request(QuestionShaping::PerCandidateRequest, 3),
                InjectionDeadline::from_duration(Duration::from_millis(5)),
            )
            .await;
        assert_eq!(traced.outcome, Err(SemanticFailure::Unauthorized));
        let service = traced.trace.service.expect("service trace");
        assert_eq!(service.attempt_count, 3);
        assert_eq!(service.attempt_latency_micros.len(), 3);
        assert_eq!(
            service.usage_parsed,
            Some(ParsedUsage {
                input_tokens: 24,
                output_tokens: 6
            })
        );
        assert_eq!(service.resolved_model_ids.len(), 2);
    }

    #[tokio::test]
    async fn empty_candidate_request_does_not_call_vendor() {
        let stub = start_stub(vec![], vec![], Duration::ZERO).await;
        let judge = remote_judge(stub.endpoint, Duration::from_millis(100), 1, 2);
        let traced = judge
            .score_pairs(
                request(QuestionShaping::SharedStateQuestions, 0),
                InjectionDeadline::from_duration(Duration::from_millis(5)),
            )
            .await;
        assert_eq!(traced.outcome, Ok(Vec::new()));
        assert_eq!(stub.state.request_count.load(Ordering::SeqCst), 0);
        assert_eq!(
            traced.trace.service.expect("service trace").attempt_count,
            0
        );
    }

    #[tokio::test]
    async fn generated_artifact_uses_a_real_failed_call_and_parses_back() {
        let stub = start_stub(
            vec![StatusCode::UNPROCESSABLE_ENTITY],
            vec![],
            Duration::ZERO,
        )
        .await;
        let judge = remote_judge(stub.endpoint, Duration::from_millis(100), 1, 2);
        let scoring_request = request(QuestionShaping::SharedStateQuestions, 1);
        let traced = judge
            .score_pairs(
                scoring_request.clone(),
                InjectionDeadline::from_duration(Duration::from_millis(5)),
            )
            .await;
        let service = traced.trace.service.as_ref().expect("service trace");
        let completed = RelevanceJudgmentCompletedRecord {
            invocation_id: traced.trace.invocation_id.clone(),
            identity: InvocationIdentity {
                qsf_session_id: "session".to_owned(),
                attachment_epoch: 1,
                exchange_index: 2,
                input_revision: 0,
                invocation_seq: 1,
            },
            candidates: completed_candidates(&scoring_request, &traced),
            latency_micros: traced.trace.latency_micros,
            attempt_count: service.attempt_count,
            retry_reasons: service.retry_reasons.clone(),
            usage_raw: service.usage_raw.clone(),
            usage_parsed: service.usage_parsed.clone(),
            failure_reason: traced.trace.failure.clone(),
            completion_disposition: CompletionDisposition::LateDropped,
        };
        let record = SemanticLifecycleRecord::RelevanceJudgmentCompleted(completed);
        let directory = tempfile::tempdir().expect("temporary artifact directory");
        let path = directory.path().join("semantic-trace.jsonl");
        fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&record).expect("serialize")),
        )
        .expect("write generated artifact");
        let parsed: SemanticLifecycleRecord = serde_json::from_str(
            fs::read_to_string(path)
                .expect("read generated artifact")
                .trim(),
        )
        .expect("parse generated artifact");
        assert!(matches!(
            parsed,
            SemanticLifecycleRecord::RelevanceJudgmentCompleted(record)
                if matches!(record.failure_reason, Some(SemanticFailure::InvalidRequest { .. }))
                    && record.candidates.len() == 1
                    && !record.candidates[0].judged
        ));

        let serialized_trace = serde_json::to_string(&traced.trace).expect("serialize trace");
        let parsed_trace: SemanticTraceRecord =
            serde_json::from_str(&serialized_trace).expect("round trip trace");
        assert_eq!(parsed_trace, traced.trace);
    }

    #[tokio::test]
    async fn request_timeout_is_traced_as_a_fully_formed_failure() {
        let stub = start_stub(
            vec![StatusCode::OK],
            vec!["jev-1.13.0-resolved"],
            Duration::from_millis(70),
        )
        .await;
        let judge = remote_judge(stub.endpoint, Duration::from_millis(20), 1, 2);
        let traced = judge
            .score_pairs(
                request(QuestionShaping::SharedStateQuestions, 1),
                InjectionDeadline::from_duration(Duration::from_millis(5)),
            )
            .await;
        assert_eq!(traced.outcome, Err(SemanticFailure::Timeout));
        assert_eq!(traced.trace.failure, Some(SemanticFailure::Timeout));
        assert_eq!(traced.trace.backend_kind, BackendKind::RemoteHttp);
        assert!(traced.trace.service.is_some());
    }
}
