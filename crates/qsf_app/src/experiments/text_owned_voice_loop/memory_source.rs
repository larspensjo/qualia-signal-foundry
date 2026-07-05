use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

use crate::memory::{Association, MemoryFixture, MemoryRecord, phase_four_fixture};

const VOICE_MEMORY_SOURCE_ENV_VAR: &str = "QSF_VOICE_MEMORY_SOURCE";
const VOICE_MEMORY_FILE_ENV_VAR: &str = "QSF_VOICE_MEMORY_FILE";

pub(crate) trait VoiceLoopMemorySource {
    fn load(&self) -> anyhow::Result<VoiceMemorySourceSnapshot>;
}

pub(crate) struct SharedVoiceMemorySource {
    state_dir: PathBuf,
}

impl SharedVoiceMemorySource {
    pub(crate) fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
    }
}

impl VoiceLoopMemorySource for SharedVoiceMemorySource {
    fn load(&self) -> anyhow::Result<VoiceMemorySourceSnapshot> {
        let memory_store_path = self.state_dir.join("memory-store.json");
        let store = crate::memory::MemoryStore::load_or_empty(&memory_store_path)?;
        Ok(VoiceMemorySourceSnapshot::from_memory_store(
            &memory_store_path,
            store.contents().clone(),
        ))
    }
}

pub(crate) struct PhaseFourVoiceMemorySource;

impl VoiceLoopMemorySource for PhaseFourVoiceMemorySource {
    fn load(&self) -> anyhow::Result<VoiceMemorySourceSnapshot> {
        Ok(VoiceMemorySourceSnapshot::from_fixture(
            "phase_four_fixture",
            "crate::memory::phase_four_fixture",
            phase_four_fixture(),
        ))
    }
}

pub(crate) struct FileVoiceMemorySource {
    path: PathBuf,
}

impl FileVoiceMemorySource {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl VoiceLoopMemorySource for FileVoiceMemorySource {
    fn load(&self) -> anyhow::Result<VoiceMemorySourceSnapshot> {
        let contents = fs::read_to_string(&self.path).with_context(|| {
            format!(
                "failed to read voice memory source file `{}`",
                self.path.display()
            )
        })?;
        let fixture: MemoryFixture = serde_json::from_str(&contents).with_context(|| {
            format!(
                "failed to parse voice memory source file `{}`",
                self.path.display()
            )
        })?;

        Ok(VoiceMemorySourceSnapshot::from_fixture(
            "file",
            self.path.display().to_string(),
            fixture,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct VoiceMemorySourceSnapshot {
    pub(crate) source_name: String,
    pub(crate) source_reference: String,
    pub(crate) records: Vec<MemoryRecord>,
    pub(crate) associations: Vec<Association>,
}

impl VoiceMemorySourceSnapshot {
    fn from_fixture(
        source_name: impl Into<String>,
        source_reference: impl Into<String>,
        fixture: MemoryFixture,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            source_reference: source_reference.into(),
            records: fixture.records,
            associations: fixture.associations,
        }
    }

    fn from_memory_store(path: &Path, contents: crate::memory::MemoryStoreContents) -> Self {
        Self {
            source_name: "memory_store".to_string(),
            source_reference: path.display().to_string(),
            records: contents.records,
            associations: contents.associations,
        }
    }

    pub(crate) fn record_count(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn association_count(&self) -> usize {
        self.associations.len()
    }
}

pub(crate) fn build_voice_memory_source_from_env(
    state_dir: &Path,
) -> anyhow::Result<Box<dyn VoiceLoopMemorySource>> {
    match std::env::var(VOICE_MEMORY_SOURCE_ENV_VAR)
        .unwrap_or_else(|_| "memory_store".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "memory_store" | "memory-store" | "shared" => {
            Ok(Box::new(SharedVoiceMemorySource::new(state_dir)))
        }
        "phase_four_fixture" | "fixture" => Ok(Box::new(PhaseFourVoiceMemorySource)),
        "file" => {
            let path = std::env::var(VOICE_MEMORY_FILE_ENV_VAR).with_context(|| {
                format!(
                    "`{VOICE_MEMORY_FILE_ENV_VAR}` must be set when `{VOICE_MEMORY_SOURCE_ENV_VAR}=file`"
                )
            })?;
            Ok(Box::new(FileVoiceMemorySource::new(path)))
        }
        value => anyhow::bail!(
            "unsupported voice memory source `{}`; expected `memory_store`, `phase_four_fixture`, or `file`",
            value
        ),
    }
}
