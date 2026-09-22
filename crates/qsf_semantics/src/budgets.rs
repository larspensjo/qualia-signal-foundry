//! Shared timing budgets for relevance judgment.

use std::time::Duration;

use thiserror::Error;

/// The maximum p95 time relevance work may add before the first audio begins.
pub const MAX_ADDED_TIME_TO_FIRST_AUDIO_MS: u64 = 300;
/// Default duration for waiting to use a verdict in the current turn.
pub const DEFAULT_INJECTION_DEADLINE_MS: u64 = 200;
/// Default ceiling for an individual hosted-judge request.
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 1_000;
/// Default maximum hosted-service attempts, including the first request.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;
/// Default initial delay before retrying a retryable hosted-service response.
pub const DEFAULT_INITIAL_BACKOFF_MS: u64 = 25;
/// Default upper bound for exponential hosted-service retry delay.
pub const DEFAULT_MAX_BACKOFF_MS: u64 = 200;
/// Default maximum number of in-flight hosted requests.
pub const DEFAULT_MAX_CONCURRENCY: usize = 4;

/// Violation of the relevance-judgment timing contract.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TimingBudgetError {
    /// The request would be cancelled at or before the carry-over boundary.
    #[error("request timeout must be strictly greater than injection deadline")]
    RequestTimeoutNotAfterInjectionDeadline,
    /// Waiting for injection would exceed the fixed added-latency budget.
    #[error(
        "injection deadline must not exceed the {MAX_ADDED_TIME_TO_FIRST_AUDIO_MS} ms added-time budget"
    )]
    InjectionDeadlineExceedsAddedTimeBudget,
}

/// Returns the default injection deadline.
pub const fn default_injection_deadline() -> Duration {
    Duration::from_millis(DEFAULT_INJECTION_DEADLINE_MS)
}

/// Returns the default request timeout.
pub const fn default_request_timeout() -> Duration {
    Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS)
}

/// Returns whether the injection wait fits the fixed first-audio budget.
pub fn injection_deadline_fits_budget(injection_deadline: Duration) -> bool {
    injection_deadline <= Duration::from_millis(MAX_ADDED_TIME_TO_FIRST_AUDIO_MS)
}

/// Validates the fixed injection budget and the request carry-over relationship.
pub fn validate_timing_budget(
    injection_deadline: Duration,
    request_timeout: Duration,
) -> Result<(), TimingBudgetError> {
    if !injection_deadline_fits_budget(injection_deadline) {
        return Err(TimingBudgetError::InjectionDeadlineExceedsAddedTimeBudget);
    }
    if request_timeout <= injection_deadline {
        return Err(TimingBudgetError::RequestTimeoutNotAfterInjectionDeadline);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_timeout_outlasts_the_injection_deadline() {
        assert_eq!(
            validate_timing_budget(default_injection_deadline(), default_request_timeout()),
            Ok(())
        );
        assert!(injection_deadline_fits_budget(default_injection_deadline()));
        assert_eq!(
            validate_timing_budget(Duration::from_millis(301), Duration::from_millis(1_000)),
            Err(TimingBudgetError::InjectionDeadlineExceedsAddedTimeBudget)
        );
        assert_eq!(
            validate_timing_budget(Duration::from_millis(200), Duration::from_millis(200)),
            Err(TimingBudgetError::RequestTimeoutNotAfterInjectionDeadline)
        );
    }
}
