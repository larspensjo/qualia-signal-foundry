//! Explicit relevance-judge backend configuration.

use std::{env, time::Duration};

use thiserror::Error;

use crate::{
    backends::remote_http::RemoteRelevanceJudgeConfig,
    budgets::{
        DEFAULT_INITIAL_BACKOFF_MS, DEFAULT_MAX_ATTEMPTS, DEFAULT_MAX_BACKOFF_MS,
        DEFAULT_MAX_CONCURRENCY, default_injection_deadline, default_request_timeout,
        validate_timing_budget,
    },
};

/// Explicitly selectable backend names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelevanceJudgeBackendName {
    /// Deterministic no-cost fixture.
    Fixture,
    /// Hosted System One service.
    RemoteHttp,
}

/// Configuration error reported instead of silently falling back to fixture.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SemanticConfigError {
    /// Backend name is not supported.
    #[error("unknown relevance judge backend `{value}`")]
    UnknownBackend {
        /// Value supplied through configuration.
        value: String,
    },
    /// Selected remote backend lacks a required value.
    #[error("remote_http relevance judge requires `{name}`")]
    MissingPrerequisite {
        /// Required configuration or environment variable.
        name: &'static str,
    },
    /// A millisecond duration override is not an unsigned integer.
    #[error(
        "invalid duration for `{name}`: `{value}` (expected milliseconds as an unsigned integer)"
    )]
    InvalidDuration {
        /// Configuration or environment variable name.
        name: &'static str,
        /// Invalid supplied value.
        value: String,
    },
    /// A count override is not a positive integer of the required size.
    #[error("invalid value for `{name}`: `{value}` (expected a positive integer)")]
    InvalidValue {
        /// Configuration or environment variable name.
        name: &'static str,
        /// Invalid supplied value.
        value: String,
    },
    /// Timer values violate the carry-over contract.
    #[error("invalid relevance-judge timing: {detail}")]
    InvalidTimerRelationship {
        /// Timing contract violation.
        detail: String,
    },
}

/// Validated relevance-judge settings.
#[derive(Clone, Debug)]
pub struct RelevanceJudgeConfig {
    /// Selected backend, defaulting only when backend name is absent.
    pub backend: RelevanceJudgeBackendName,
    /// Deadline used by callers to stop waiting for same-turn injection.
    pub injection_deadline: Duration,
    /// HTTP request timeout, which must outlast injection waiting.
    pub request_timeout: Duration,
    /// Settings present only for the selected remote backend.
    pub remote: Option<RemoteRelevanceJudgeConfig>,
}

impl RelevanceJudgeConfig {
    /// Loads explicit backend settings from process environment.
    pub fn from_env() -> Result<Self, SemanticConfigError> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    fn from_lookup(
        lookup: impl Fn(&'static str) -> Option<String>,
    ) -> Result<Self, SemanticConfigError> {
        let backend = parse_backend_name(lookup("QSF_RELEVANCE_JUDGE_BACKEND").as_deref())?;
        let injection_deadline = duration_value(
            &lookup,
            "QSF_RELEVANCE_JUDGE_INJECTION_DEADLINE_MS",
            default_injection_deadline(),
        )?;
        let request_timeout = duration_value(
            &lookup,
            "QSF_RELEVANCE_JUDGE_REQUEST_TIMEOUT_MS",
            default_request_timeout(),
        )?;
        validate_timing_budget(injection_deadline, request_timeout).map_err(|error| {
            SemanticConfigError::InvalidTimerRelationship {
                detail: error.to_string(),
            }
        })?;
        let remote = if backend == RelevanceJudgeBackendName::RemoteHttp {
            let base_url = required_value(&lookup, "QSF_RELEVANCE_JUDGE_BASE_URL")?;
            let api_key = required_value(&lookup, "TYPESAFE_API_KEY")?;
            let pinned_model_id = required_value(&lookup, "QSF_RELEVANCE_JUDGE_MODEL")?;
            Some(RemoteRelevanceJudgeConfig {
                base_url,
                api_key,
                pinned_model_id,
                max_attempts: positive_u32_value(
                    &lookup,
                    "QSF_RELEVANCE_JUDGE_MAX_ATTEMPTS",
                    DEFAULT_MAX_ATTEMPTS,
                )?,
                initial_backoff: duration_value(
                    &lookup,
                    "QSF_RELEVANCE_JUDGE_INITIAL_BACKOFF_MS",
                    Duration::from_millis(DEFAULT_INITIAL_BACKOFF_MS),
                )?,
                max_backoff: duration_value(
                    &lookup,
                    "QSF_RELEVANCE_JUDGE_MAX_BACKOFF_MS",
                    Duration::from_millis(DEFAULT_MAX_BACKOFF_MS),
                )?,
                request_timeout,
                max_concurrency: positive_usize_value(
                    &lookup,
                    "QSF_RELEVANCE_JUDGE_MAX_CONCURRENCY",
                    DEFAULT_MAX_CONCURRENCY,
                )?,
            })
        } else {
            None
        };
        Ok(Self {
            backend,
            injection_deadline,
            request_timeout,
            remote,
        })
    }
}

/// Parses the configured backend name, defaulting only when the value is absent.
pub fn parse_backend_name(
    value: Option<&str>,
) -> Result<RelevanceJudgeBackendName, SemanticConfigError> {
    match value {
        None | Some("fixture") => Ok(RelevanceJudgeBackendName::Fixture),
        Some("remote_http") => Ok(RelevanceJudgeBackendName::RemoteHttp),
        Some(value) => Err(SemanticConfigError::UnknownBackend {
            value: value.to_owned(),
        }),
    }
}

fn duration_value(
    lookup: &impl Fn(&'static str) -> Option<String>,
    name: &'static str,
    default: Duration,
) -> Result<Duration, SemanticConfigError> {
    let Some(value) = lookup(name) else {
        return Ok(default);
    };
    value
        .parse::<u64>()
        .map(Duration::from_millis)
        .map_err(|_| SemanticConfigError::InvalidDuration { name, value })
}

fn positive_u32_value(
    lookup: &impl Fn(&'static str) -> Option<String>,
    name: &'static str,
    default: u32,
) -> Result<u32, SemanticConfigError> {
    let Some(value) = lookup(name) else {
        return Ok(default);
    };
    value
        .parse::<u32>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .ok_or(SemanticConfigError::InvalidValue { name, value })
}

fn positive_usize_value(
    lookup: &impl Fn(&'static str) -> Option<String>,
    name: &'static str,
    default: usize,
) -> Result<usize, SemanticConfigError> {
    let Some(value) = lookup(name) else {
        return Ok(default);
    };
    value
        .parse::<usize>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .ok_or(SemanticConfigError::InvalidValue { name, value })
}

fn required_value(
    lookup: &impl Fn(&'static str) -> Option<String>,
    name: &'static str,
) -> Result<String, SemanticConfigError> {
    lookup(name)
        .filter(|value| !value.trim().is_empty())
        .ok_or(SemanticConfigError::MissingPrerequisite { name })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn load(entries: &[(&'static str, &str)]) -> Result<RelevanceJudgeConfig, SemanticConfigError> {
        let values = entries
            .iter()
            .map(|(name, value)| (*name, (*value).to_owned()))
            .collect::<BTreeMap<_, _>>();
        RelevanceJudgeConfig::from_lookup(|name| values.get(name).cloned())
    }

    #[test]
    fn absent_backend_defaults_to_fixture() {
        let config = load(&[]).expect("fixture defaults");
        assert_eq!(config.backend, RelevanceJudgeBackendName::Fixture);
        assert!(config.remote.is_none());
    }

    #[test]
    fn api_key_presence_never_selects_the_remote_backend() {
        let config = load(&[("TYPESAFE_API_KEY", "present-but-not-selected")])
            .expect("fixture remains selected");
        assert_eq!(config.backend, RelevanceJudgeBackendName::Fixture);
        assert!(config.remote.is_none());
    }

    #[test]
    fn backend_name_parser_rejects_typos() {
        assert_eq!(
            parse_backend_name(Some("remote-http")),
            Err(SemanticConfigError::UnknownBackend {
                value: "remote-http".to_owned()
            })
        );
    }

    #[test]
    fn selected_remote_backend_requires_its_api_key() {
        assert_eq!(
            load(&[
                ("QSF_RELEVANCE_JUDGE_BACKEND", "remote_http"),
                ("QSF_RELEVANCE_JUDGE_BASE_URL", "https://example.invalid"),
                ("QSF_RELEVANCE_JUDGE_MODEL", "jev-1.13.0"),
            ])
            .expect_err("missing API key"),
            SemanticConfigError::MissingPrerequisite {
                name: "TYPESAFE_API_KEY"
            }
        );
    }

    #[test]
    fn invalid_durations_and_timer_relationships_are_loud() {
        assert_eq!(
            load(&[("QSF_RELEVANCE_JUDGE_INJECTION_DEADLINE_MS", "250ms")])
                .expect_err("malformed duration"),
            SemanticConfigError::InvalidDuration {
                name: "QSF_RELEVANCE_JUDGE_INJECTION_DEADLINE_MS",
                value: "250ms".to_owned(),
            }
        );
        assert!(matches!(
            load(&[
                ("QSF_RELEVANCE_JUDGE_INJECTION_DEADLINE_MS", "250"),
                ("QSF_RELEVANCE_JUDGE_REQUEST_TIMEOUT_MS", "250"),
            ]),
            Err(SemanticConfigError::InvalidTimerRelationship { .. })
        ));
        assert!(matches!(
            load(&[("QSF_RELEVANCE_JUDGE_INJECTION_DEADLINE_MS", "5000")]),
            Err(SemanticConfigError::InvalidTimerRelationship { .. })
        ));
    }

    #[test]
    fn remote_budget_overrides_are_parsed() {
        let config = load(&[
            ("QSF_RELEVANCE_JUDGE_BACKEND", "remote_http"),
            ("QSF_RELEVANCE_JUDGE_BASE_URL", "https://example.invalid"),
            ("TYPESAFE_API_KEY", "secret"),
            ("QSF_RELEVANCE_JUDGE_MODEL", "jev-1.13.0"),
            ("QSF_RELEVANCE_JUDGE_MAX_ATTEMPTS", "5"),
            ("QSF_RELEVANCE_JUDGE_INITIAL_BACKOFF_MS", "12"),
            ("QSF_RELEVANCE_JUDGE_MAX_BACKOFF_MS", "180"),
            ("QSF_RELEVANCE_JUDGE_MAX_CONCURRENCY", "7"),
        ])
        .expect("remote config");
        let remote = config.remote.expect("remote settings");
        assert_eq!(remote.max_attempts, 5);
        assert_eq!(remote.initial_backoff, Duration::from_millis(12));
        assert_eq!(remote.max_backoff, Duration::from_millis(180));
        assert_eq!(remote.max_concurrency, 7);
        assert_eq!(
            load(&[
                ("QSF_RELEVANCE_JUDGE_BACKEND", "remote_http"),
                ("QSF_RELEVANCE_JUDGE_BASE_URL", "https://example.invalid"),
                ("TYPESAFE_API_KEY", "secret"),
                ("QSF_RELEVANCE_JUDGE_MODEL", "jev-1.13.0"),
                ("QSF_RELEVANCE_JUDGE_MAX_ATTEMPTS", "0"),
            ])
            .expect_err("zero attempts"),
            SemanticConfigError::InvalidValue {
                name: "QSF_RELEVANCE_JUDGE_MAX_ATTEMPTS",
                value: "0".to_owned(),
            }
        );
    }
}
