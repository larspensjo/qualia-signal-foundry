use std::path::PathBuf;

/// Environment variable that points at a producer-owned corpus output directory.
pub const WORLD_CORPUS_PATH_ENV_VAR: &str = "QSF_WORLD_CORPUS_PATH";

/// How a corpus path was selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorpusPathSource {
    /// A usable caller-provided path was selected.
    Configured,
    /// No configured path was provided, so the bundled fixture is active.
    BundledFixture,
    /// A configured path did not exist, so the bundled fixture is active with a degradation.
    BundledFixtureAfterMissingConfiguredPath,
}

/// A selected corpus path and any non-fatal degradation that led to it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusPathResolution {
    /// The path the consumer should ingest.
    pub corpus_path: PathBuf,
    /// Why that path was selected.
    pub source: CorpusPathSource,
    /// An operator-visible reason when a configured corpus was unavailable.
    pub degraded_reason: Option<String>,
}

/// Returns the corpus fixture that ships with this crate.
pub fn bundled_fixture_corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus")
}

/// Resolves a configured path, falling back to the bundled fixture when it is absent or missing.
pub fn resolve_corpus_path(configured_path: Option<PathBuf>) -> CorpusPathResolution {
    match configured_path {
        Some(path) if path.is_dir() => CorpusPathResolution {
            corpus_path: path,
            source: CorpusPathSource::Configured,
            degraded_reason: None,
        },
        Some(path) => CorpusPathResolution {
            corpus_path: bundled_fixture_corpus_path(),
            source: CorpusPathSource::BundledFixtureAfterMissingConfiguredPath,
            degraded_reason: Some(format!(
                "configured world corpus path is unavailable: {}",
                path.display()
            )),
        },
        None => CorpusPathResolution {
            corpus_path: bundled_fixture_corpus_path(),
            source: CorpusPathSource::BundledFixture,
            degraded_reason: None,
        },
    }
}
