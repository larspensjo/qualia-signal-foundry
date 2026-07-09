use std::fs;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Highest producer schema version this crate can safely read.
pub const CORPUS_SUPPORTED_SCHEMA_VERSION: u32 = 1;

const CORPUS_MARKER_FILE: &str = "harvester-corpus.json";

/// A schema compatibility signal carried through ingestion reports.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CorpusSchemaDrift {
    /// The producer schema exactly matches the supported schema.
    None,
    /// The producer is older. QSF continues with an explicit degraded signal.
    Older {
        /// Schema version in the producer marker.
        found: u32,
        /// Schema version understood by QSF.
        supported: u32,
    },
    /// The producer is newer. QSF must refuse to ingest it.
    Newer {
        /// Schema version in the producer marker.
        found: u32,
        /// Schema version understood by QSF.
        supported: u32,
    },
}

/// The validated producer marker that defines which article files are in the corpus.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CorpusMarker {
    /// Producer schema version.
    pub schema_version: u32,
    /// Producer identifier, carried into ingestion reports for provenance.
    pub producer: String,
    /// Relative glob patterns for article files.
    pub article_patterns: Vec<String>,
    /// Relative directories or glob patterns that must never be treated as articles.
    pub generated_artifacts: Vec<String>,
    /// Relative directories or glob patterns holding producer-private state.
    pub internal_state: Vec<String>,
}

/// A marker parsing or validation failure.
#[derive(Debug, Error)]
pub enum CorpusMarkerError {
    /// The marker could not be read.
    #[error("failed to read corpus marker {path}: {source}")]
    Read {
        /// Marker path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// The marker was not valid JSON in the supported shape.
    #[error("failed to parse corpus marker {path}: {source}")]
    Parse {
        /// Marker path.
        path: PathBuf,
        /// JSON parsing error.
        source: serde_json::Error,
    },
    /// A marker path would escape the corpus root or is otherwise unsafe.
    #[error("corpus marker contains unsafe {field} path or pattern: {value}")]
    UnsafePath {
        /// Marker field that contained the unsafe value.
        field: &'static str,
        /// Unsafe input from the marker.
        value: String,
    },
    /// The marker contained a glob that could not be compiled.
    #[error("corpus marker contains invalid {field} glob {value}: {source}")]
    InvalidGlob {
        /// Marker field that contained the invalid glob.
        field: &'static str,
        /// Invalid glob source.
        value: String,
        /// Glob parsing failure.
        source: globset::Error,
    },
}

#[derive(Debug, Deserialize)]
struct MarkerDocument {
    schema_version: u32,
    producer: MarkerProducer,
    layout: MarkerLayout,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MarkerProducer {
    Name(String),
    Details {
        name: String,
        #[serde(default, rename = "crate")]
        crate_name: Option<String>,
        #[serde(default)]
        crate_version: Option<String>,
    },
}

impl MarkerProducer {
    fn label(self) -> String {
        match self {
            Self::Name(name) => name,
            Self::Details {
                name,
                crate_name,
                crate_version,
            } => match (crate_name, crate_version) {
                (Some(crate_name), Some(crate_version)) => {
                    format!("{name} ({crate_name} {crate_version})")
                }
                _ => name,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct MarkerLayout {
    articles: Vec<String>,
    #[serde(default)]
    generated_artifacts: Vec<String>,
    #[serde(default)]
    internal_state: Vec<String>,
}

impl CorpusMarker {
    /// Returns the schema compatibility signal for this marker.
    pub fn schema_drift(&self) -> CorpusSchemaDrift {
        match self.schema_version.cmp(&CORPUS_SUPPORTED_SCHEMA_VERSION) {
            std::cmp::Ordering::Equal => CorpusSchemaDrift::None,
            std::cmp::Ordering::Less => CorpusSchemaDrift::Older {
                found: self.schema_version,
                supported: CORPUS_SUPPORTED_SCHEMA_VERSION,
            },
            std::cmp::Ordering::Greater => CorpusSchemaDrift::Newer {
                found: self.schema_version,
                supported: CORPUS_SUPPORTED_SCHEMA_VERSION,
            },
        }
    }

    pub(crate) fn article_matcher(&self) -> Result<GlobSet, CorpusMarkerError> {
        compile_globs("layout.articles", &self.article_patterns)
    }

    pub(crate) fn excluded_matcher(&self) -> Result<GlobSet, CorpusMarkerError> {
        let patterns = self
            .generated_artifacts
            .iter()
            .chain(&self.internal_state)
            .flat_map(|value| [value.clone(), format!("{}/**", value.trim_end_matches('/'))])
            .collect::<Vec<_>>();
        compile_globs("layout generated/internal paths", &patterns)
    }
}

/// Reads and validates `harvester-corpus.json` from a corpus output directory.
pub fn read_marker(corpus_root: &Path) -> Result<CorpusMarker, CorpusMarkerError> {
    let path = corpus_root.join(CORPUS_MARKER_FILE);
    let raw = fs::read_to_string(&path).map_err(|source| CorpusMarkerError::Read {
        path: path.clone(),
        source,
    })?;
    let document: MarkerDocument =
        serde_json::from_str(&raw).map_err(|source| CorpusMarkerError::Parse {
            path: path.clone(),
            source,
        })?;

    validate_marker_paths("layout.articles", &document.layout.articles)?;
    validate_marker_paths(
        "layout.generated_artifacts",
        &document.layout.generated_artifacts,
    )?;
    validate_marker_paths("layout.internal_state", &document.layout.internal_state)?;

    Ok(CorpusMarker {
        schema_version: document.schema_version,
        producer: document.producer.label(),
        article_patterns: document.layout.articles,
        generated_artifacts: document.layout.generated_artifacts,
        internal_state: document.layout.internal_state,
    })
}

fn validate_marker_paths(field: &'static str, values: &[String]) -> Result<(), CorpusMarkerError> {
    for value in values {
        let normalized = value.replace('\\', "/");
        let path = Path::new(&normalized);
        if normalized.is_empty()
            || path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(CorpusMarkerError::UnsafePath {
                field,
                value: value.clone(),
            });
        }
    }
    Ok(())
}

fn compile_globs(field: &'static str, patterns: &[String]) -> Result<GlobSet, CorpusMarkerError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let normalized = pattern.replace('\\', "/");
        let glob = Glob::new(&normalized).map_err(|source| CorpusMarkerError::InvalidGlob {
            field,
            value: pattern.clone(),
            source,
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|source| CorpusMarkerError::InvalidGlob {
            field,
            value: "<combined patterns>".to_string(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{CORPUS_SUPPORTED_SCHEMA_VERSION, CorpusSchemaDrift, read_marker};

    #[test]
    fn newer_schema_produces_a_refusal_signal() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("harvester-corpus.json"),
            format!(
                r#"{{"schema_version":{},"producer":"test","layout":{{"articles":["*.md"]}}}}"#,
                CORPUS_SUPPORTED_SCHEMA_VERSION + 1
            ),
        )
        .unwrap();

        let marker = read_marker(directory.path()).unwrap();
        assert!(matches!(
            marker.schema_drift(),
            CorpusSchemaDrift::Newer { .. }
        ));
    }

    #[test]
    fn older_schema_produces_a_degraded_signal() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("harvester-corpus.json"),
            r#"{"schema_version":0,"producer":"test","layout":{"articles":["*.md"]}}"#,
        )
        .unwrap();

        let marker = read_marker(directory.path()).unwrap();
        assert!(matches!(
            marker.schema_drift(),
            CorpusSchemaDrift::Older { .. }
        ));
    }

    #[test]
    fn producer_object_is_accepted_and_labeled() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("harvester-corpus.json"),
            r#"{
  "schema_version": 1,
  "producer": {"name":"harvester","crate":"harvester_engine","crate_version":"0.1.0"},
  "layout": {"articles":["*.md"]}
}"#,
        )
        .unwrap();

        let marker = read_marker(directory.path()).unwrap();

        assert_eq!(marker.producer, "harvester (harvester_engine 0.1.0)");
    }
}
