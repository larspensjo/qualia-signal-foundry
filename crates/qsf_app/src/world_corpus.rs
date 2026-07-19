//! Application-owned configuration and operator command for the world corpus index.

use std::path::PathBuf;

use anyhow::{Context, Result};
use qsf_corpus::{
    CorpusIngestReport, CorpusPathResolution, CorpusPathSource, CorpusSchemaDrift,
    WORLD_CORPUS_PATH_ENV_VAR, bundled_fixture_corpus_path, load_ledger, read_marker,
    refresh_corpus, resolve_corpus_path, write_ledger,
};

/// Default persisted content-hash ledger and corpus-index artifact path.
pub const DEFAULT_WORLD_CORPUS_LEDGER_PATH: &str = "state/world-corpus/index.json";

/// Input accepted by the first-class world corpus ingestion command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldCorpusIngestOptions {
    /// An explicit producer output directory. This never falls back silently.
    pub corpus_path: Option<PathBuf>,
    /// Location of the persisted content-hash ledger.
    pub ledger_path: PathBuf,
}

/// Result rendered by the first-class world corpus ingestion command.
#[derive(Clone, Debug)]
pub struct WorldCorpusIngestSummary {
    /// Selected path and source.
    pub path_resolution: CorpusPathResolution,
    /// Location of the persisted content-hash ledger.
    pub ledger_path: PathBuf,
    /// Refresh counts and schema compatibility signal.
    pub report: CorpusIngestReport,
    /// Query latency for a deterministic command-level probe.
    pub query_probe_latency_ms: u64,
    /// Query latency for a deterministic command-level probe.
    pub query_probe_latency_ns: u64,
}

/// Refreshes the shared corpus ledger for a caller that needs the in-memory index as well as the
/// persisted artifact. Sleep world-memory consolidation uses this instead of duplicating corpus
/// path resolution or content-hash refresh behavior.
pub fn refresh_world_corpus(
    corpus_path: Option<PathBuf>,
    ledger_path: PathBuf,
) -> Result<(CorpusPathResolution, qsf_corpus::CorpusRefresh)> {
    let previous_ledger = load_ledger(&ledger_path)?;
    let (path_resolution, refresh) = match corpus_path {
        Some(corpus_path) => {
            let resolution = CorpusPathResolution {
                corpus_path,
                source: CorpusPathSource::Configured,
                degraded_reason: None,
            };
            let refresh = refresh_corpus(&resolution.corpus_path, previous_ledger.as_ref())?;
            (resolution, refresh)
        }
        None => refresh_configured_or_fixture(previous_ledger.as_ref())?,
    };
    write_ledger(&ledger_path, &refresh.ledger)?;
    Ok((path_resolution, refresh))
}

impl WorldCorpusIngestSummary {
    /// Produces a compact, operator-readable report without hiding degraded operation.
    pub fn render(&self) -> String {
        let source = match self.path_resolution.source {
            CorpusPathSource::Configured => "configured path",
            CorpusPathSource::BundledFixture => "bundled fixture",
            CorpusPathSource::BundledFixtureAfterMissingConfiguredPath => {
                "bundled fixture after configured-path degradation"
            }
        };
        let drift = match &self.report.schema_drift {
            CorpusSchemaDrift::None => "none".to_string(),
            CorpusSchemaDrift::Older { found, supported } => {
                format!("older producer schema {found}; supported schema {supported}")
            }
            CorpusSchemaDrift::Newer { found, supported } => {
                format!("newer producer schema {found}; supported schema {supported}")
            }
        };
        let degradation = self
            .path_resolution
            .degraded_reason
            .as_deref()
            .unwrap_or("none");

        format!(
            "World corpus ingest completed.\n  corpus: {} ({source})\n  ledger: {}\n  producer/schema: {} / {}\n  schema drift: {drift}\n  degradation: {degradation}\n  files/indexed: {} / {}\n  added/reused/changed/removed/skipped: {} / {} / {} / {} / {}\n  index build: {} ms\n  query probe: {} ms",
            self.path_resolution.corpus_path.display(),
            self.ledger_path.display(),
            self.report.producer,
            self.report.schema_version,
            self.report.article_files_enumerated,
            self.report.articles_indexed,
            self.report.articles_added,
            self.report.articles_reused,
            self.report.articles_changed,
            self.report.articles_removed,
            self.report.articles_skipped,
            self.report.index_build_latency_ms,
            self.query_probe_latency_ms,
        )
    }
}

/// Refreshes and persists the external world corpus index.
///
/// An explicit `--corpus-path` is an operator request and fails loudly. The ordinary runtime
/// configuration instead uses `QSF_WORLD_CORPUS_PATH`, falling back to the bundled fixture for a
/// missing or unreadable configured path while retaining a visible degradation reason.
pub fn ingest_world_corpus(options: WorldCorpusIngestOptions) -> Result<WorldCorpusIngestSummary> {
    let (path_resolution, refresh) =
        refresh_world_corpus(options.corpus_path, options.ledger_path.clone())?;

    for issue in &refresh.report.issues {
        engine_logging::engine_warn!(
            "world corpus skipped article path={} reason={}",
            issue.path,
            issue.reason
        );
        eprintln!(
            "WARNING world corpus skipped article path={} reason={}",
            issue.path, issue.reason
        );
    }
    let query_probe = refresh.index.query("artificial intelligence transition", 1);

    Ok(WorldCorpusIngestSummary {
        path_resolution,
        ledger_path: options.ledger_path,
        report: refresh.report,
        query_probe_latency_ms: query_probe.latency_ms,
        query_probe_latency_ns: query_probe.latency_ns,
    })
}

fn refresh_configured_or_fixture(
    previous_ledger: Option<&qsf_corpus::CorpusLedger>,
) -> Result<(CorpusPathResolution, qsf_corpus::CorpusRefresh)> {
    let configured_path = std::env::var_os(WORLD_CORPUS_PATH_ENV_VAR)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let resolution = resolve_corpus_path(configured_path);
    match refresh_corpus(&resolution.corpus_path, previous_ledger) {
        Ok(refresh) => Ok((resolution, refresh)),
        Err(error) if resolution.source == CorpusPathSource::Configured => {
            if configured_path_has_newer_schema(&resolution.corpus_path)? {
                return Err(error).with_context(|| {
                    format!(
                        "configured world corpus {} uses a newer unsupported schema",
                        resolution.corpus_path.display()
                    )
                });
            }

            let degraded_reason = format!(
                "configured world corpus failed at {}: {}; using bundled fixture",
                resolution.corpus_path.display(),
                error
            );
            let fixture = bundled_fixture_corpus_path();
            let refresh = refresh_corpus(&fixture, previous_ledger).with_context(|| {
                format!(
                    "configured corpus failed and bundled fixture {} could not be ingested",
                    fixture.display()
                )
            })?;
            Ok((
                CorpusPathResolution {
                    corpus_path: fixture,
                    source: CorpusPathSource::BundledFixtureAfterMissingConfiguredPath,
                    degraded_reason: Some(degraded_reason),
                },
                refresh,
            ))
        }
        Err(error) => Err(error),
    }
}

fn configured_path_has_newer_schema(path: &std::path::Path) -> Result<bool> {
    match read_marker(path) {
        Ok(marker) => Ok(matches!(
            marker.schema_drift(),
            CorpusSchemaDrift::Newer { .. }
        )),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use qsf_corpus::{CorpusPathSource, bundled_fixture_corpus_path, resolve_corpus_path};

    #[test]
    fn missing_configured_path_degrades_to_the_bundled_fixture() {
        let resolution = resolve_corpus_path(Some(PathBuf::from("definitely-missing-corpus")));

        assert_eq!(
            resolution.source,
            CorpusPathSource::BundledFixtureAfterMissingConfiguredPath
        );
        assert_eq!(resolution.corpus_path, bundled_fixture_corpus_path());
        assert!(resolution.degraded_reason.is_some());
    }

    #[test]
    fn rendered_summary_keeps_schema_and_degradation_visible() {
        let directory = tempfile::tempdir().unwrap();
        let options = super::WorldCorpusIngestOptions {
            corpus_path: Some(bundled_fixture_corpus_path()),
            ledger_path: directory.path().join("index.json"),
        };
        let summary = super::ingest_world_corpus(options).unwrap();
        let rendered = summary.render();

        assert!(rendered.contains("producer/schema"));
        assert!(rendered.contains("schema drift"));
        assert!(rendered.contains("query probe"));
    }
}
