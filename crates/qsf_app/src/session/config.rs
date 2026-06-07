use std::path::PathBuf;

use super::{MemorySourceConfig, SessionConfig};

pub(crate) const DEFAULT_SESSION_MODEL: &str = "gpt-5.4-mini";
pub(crate) const DEFAULT_MAX_TURNS: usize = 10;
pub(crate) const DEFAULT_WARM_THRESHOLD: usize = 6;

const SESSION_MODEL_ENV_VAR: &str = "QSF_CONVERSATION_MODEL";
const SESSION_MAX_TURNS_ENV_VAR: &str = "QSF_SESSION_MAX_TURNS";
const SESSION_ALLOW_OVER_LIMIT_ENV_VAR: &str = "QSF_SESSION_ALLOW_OVER_LIMIT";
const SESSION_WARM_THRESHOLD_ENV_VAR: &str = "QSF_SESSION_WARM_THRESHOLD";
const SESSION_MEMORY_SOURCE_ENV_VAR: &str = "QSF_SESSION_MEMORY_SOURCE";
const SESSION_MEMORY_FILE_ENV_VAR: &str = "QSF_SESSION_MEMORY_FILE";

impl SessionConfig {
    pub(crate) fn from_env() -> Self {
        let model_id = std::env::var(SESSION_MODEL_ENV_VAR)
            .unwrap_or_else(|_| DEFAULT_SESSION_MODEL.to_string());
        let max_turns = std::env::var(SESSION_MAX_TURNS_ENV_VAR)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MAX_TURNS);
        let warm_threshold = std::env::var(SESSION_WARM_THRESHOLD_ENV_VAR)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_WARM_THRESHOLD);
        let allow_over_limit = std::env::var(SESSION_ALLOW_OVER_LIMIT_ENV_VAR)
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let memory_source = MemorySourceConfig::from_env();

        Self {
            model_id,
            max_turns,
            warm_threshold,
            allow_over_limit,
            memory_source,
        }
    }
}

impl MemorySourceConfig {
    pub(crate) fn from_env() -> Self {
        let source = std::env::var(SESSION_MEMORY_SOURCE_ENV_VAR)
            .unwrap_or_else(|_| "phase_four_fixture".to_string());
        let file = std::env::var(SESSION_MEMORY_FILE_ENV_VAR)
            .ok()
            .map(PathBuf::from);

        Self { source, file }
    }
}
