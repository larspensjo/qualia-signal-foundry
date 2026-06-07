use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::runtime::run_context::RunContext;
use crate::session::ageing::sanitize_error;

use super::{SessionMemorySource, SessionMemorySourceSnapshot};

pub(crate) const DEFAULT_TURN_MAX_OUTPUT_TOKENS: u32 = 1024;

const SESSION_MEMORY_SOURCE_ENV_VAR: &str = "QSF_SESSION_MEMORY_SOURCE";
const SESSION_MEMORY_FILE_ENV_VAR: &str = "QSF_SESSION_MEMORY_FILE";
const SESSION_TURN_MAX_OUTPUT_TOKENS_ENV_VAR: &str = "QSF_SESSION_TURN_MAX_OUTPUT_TOKENS";

pub(crate) fn turn_max_output_tokens_from_env() -> u32 {
    parse_turn_max_output_tokens(std::env::var(SESSION_TURN_MAX_OUTPUT_TOKENS_ENV_VAR).ok())
}

pub(crate) fn parse_turn_max_output_tokens(raw: Option<String>) -> u32 {
    raw.and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TURN_MAX_OUTPUT_TOKENS)
}

pub(crate) fn build_session_memory_source_from_env() -> Box<dyn SessionMemorySource> {
    let requested = std::env::var(SESSION_MEMORY_SOURCE_ENV_VAR)
        .unwrap_or_else(|_| "phase_four_fixture".to_string());
    match requested.trim().to_ascii_lowercase().as_str() {
        "file" => std::env::var(SESSION_MEMORY_FILE_ENV_VAR)
            .map(|path| {
                Box::new(FileSessionMemorySource { path: path.into() })
                    as Box<dyn SessionMemorySource>
            })
            .unwrap_or_else(|_| Box::new(MissingFileSessionMemorySource)),
        _ => Box::new(PhaseFourSessionMemorySource),
    }
}

struct PhaseFourSessionMemorySource;

impl SessionMemorySource for PhaseFourSessionMemorySource {
    fn load(&self, _context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot> {
        Ok(SessionMemorySourceSnapshot::from_fixture(
            "phase_four_fixture",
            "crate::memory::phase_four_fixture",
            crate::memory::phase_four_fixture(),
        ))
    }
}

struct FileSessionMemorySource {
    path: PathBuf,
}

impl SessionMemorySource for FileSessionMemorySource {
    fn load(&self, context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot> {
        match fs::read_to_string(&self.path)
            .with_context(|| {
                format!(
                    "failed to read session memory file `{}`",
                    self.path.display()
                )
            })
            .and_then(|contents| {
                serde_json::from_str::<crate::memory::MemoryFixture>(&contents).with_context(|| {
                    format!(
                        "failed to parse session memory file `{}`",
                        self.path.display()
                    )
                })
            }) {
            Ok(fixture) => Ok(SessionMemorySourceSnapshot::from_fixture(
                "file",
                self.path.display().to_string(),
                fixture,
            )),
            Err(error) => {
                let error_summary = sanitize_error(&error.to_string());
                context.record_event(
                    EventType::ErrorOccurred,
                    json!({
                        "stage": "session-memory-source",
                        "source": "file",
                        "path": self.path.display().to_string(),
                        "fallback": "phase_four_fixture",
                        "error": error_summary,
                    }),
                    None,
                )?;
                Ok(SessionMemorySourceSnapshot::from_fixture(
                    "phase_four_fixture",
                    "fallback_after_file_error",
                    crate::memory::phase_four_fixture(),
                ))
            }
        }
    }
}

pub(crate) struct MissingFileSessionMemorySource;

impl SessionMemorySource for MissingFileSessionMemorySource {
    fn load(&self, context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot> {
        context.record_event(
            EventType::ErrorOccurred,
            json!({
                "stage": "session-memory-source",
                "source": "file",
                "missing_env_var": SESSION_MEMORY_FILE_ENV_VAR,
                "fallback": "phase_four_fixture",
                "error": format!("`{SESSION_MEMORY_FILE_ENV_VAR}` must be set when `{SESSION_MEMORY_SOURCE_ENV_VAR}=file`"),
            }),
            None,
        )?;
        Ok(SessionMemorySourceSnapshot::from_fixture(
            "phase_four_fixture",
            "fallback_after_missing_file_env",
            crate::memory::phase_four_fixture(),
        ))
    }
}
