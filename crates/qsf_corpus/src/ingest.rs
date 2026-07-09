use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, anyhow, bail};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{
    Article, CorpusIndex, CorpusMarker, CorpusSchemaDrift, content_hash, parse_article, read_marker,
};

/// Version of the persisted corpus ledger format.
pub const INDEX_LEDGER_VERSION: u32 = 1;

/// The persisted, content-addressed article cache from which a corpus index is rebuilt.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CorpusLedger {
    /// Version of this ledger format, independent from the producer marker schema.
    pub ledger_version: u32,
    /// Corpus root whose files supplied this ledger.
    pub corpus_root: String,
    /// Marker used during the last successful refresh.
    pub marker: CorpusMarker,
    /// Content hashes by normalized corpus-relative path.
    pub content_hash_by_path: BTreeMap<String, String>,
    /// Validated articles keyed by content hash.
    pub articles_by_content_hash: BTreeMap<String, Article>,
}

impl CorpusLedger {
    fn article_for_path(&self, relative_path: &str) -> Option<&Article> {
        self.content_hash_by_path
            .get(relative_path)
            .and_then(|hash| self.articles_by_content_hash.get(hash))
    }
}

/// An individual source file skipped during ingestion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CorpusIngestIssue {
    /// Corpus-relative file path, or the source path when relative conversion was not possible.
    pub path: String,
    /// The reason the file was skipped.
    pub reason: String,
}

/// Counts and compatibility details from one corpus refresh.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CorpusIngestReport {
    /// Selected corpus root.
    pub corpus_root: String,
    /// Producer schema version.
    pub schema_version: u32,
    /// Producer identifier from the marker.
    pub producer: String,
    /// Structured producer/QSF schema compatibility signal.
    pub schema_drift: CorpusSchemaDrift,
    /// Number of files selected by `layout.articles` before parsing.
    pub article_files_enumerated: usize,
    /// Number of articles that were newly parsed and indexed.
    pub articles_added: usize,
    /// Number of previously parsed articles reused without frontmatter reparsing.
    pub articles_reused: usize,
    /// Number of same-path articles whose content hash changed and were reparsed.
    pub articles_changed: usize,
    /// Number of ledger articles no longer selected by the marker/file layout.
    pub articles_removed: usize,
    /// Files skipped because they were unreadable or malformed.
    pub articles_skipped: usize,
    /// Total validated articles available to the index after refresh.
    pub articles_indexed: usize,
    /// Time spent assembling the index after parsing/reuse.
    pub index_build_latency_ms: u64,
    /// Time spent assembling the index after parsing/reuse.
    pub index_build_latency_ns: u64,
    /// Contextual skipped-file diagnostics for operator logging.
    pub issues: Vec<CorpusIngestIssue>,
}

/// A fully refreshed corpus: persisted ledger, in-memory index, and operator report.
#[derive(Clone, Debug)]
pub struct CorpusRefresh {
    /// The refreshed content-hash ledger to persist.
    pub ledger: CorpusLedger,
    /// The queryable in-memory lexical index.
    pub index: CorpusIndex,
    /// Ingestion counts, schema drift, and skipped-file diagnostics.
    pub report: CorpusIngestReport,
}

/// Loads a persisted ledger, returning `None` when no artifact exists yet.
pub fn load_ledger(path: &Path) -> anyhow::Result<Option<CorpusLedger>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read corpus ledger {}", path.display()))?;
    let ledger: CorpusLedger = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse corpus ledger {}", path.display()))?;
    if ledger.ledger_version != INDEX_LEDGER_VERSION {
        bail!(
            "unsupported corpus ledger version {} at {}; supported version is {}",
            ledger.ledger_version,
            path.display(),
            INDEX_LEDGER_VERSION
        );
    }
    Ok(Some(ledger))
}

/// Writes a refreshed ledger to its designated persistent artifact path.
pub fn write_ledger(path: &Path, ledger: &CorpusLedger) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("corpus ledger path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create corpus ledger directory {}",
            parent.display()
        )
    })?;
    let raw = serde_json::to_vec_pretty(ledger).context("failed to serialize corpus ledger")?;
    fs::write(path, raw)
        .with_context(|| format!("failed to write corpus ledger {}", path.display()))
}

/// Refreshes a corpus against an optional prior ledger and rebuilds its in-memory index.
///
/// Files with unchanged content hashes are reused from the prior validated ledger. Removed files
/// disappear from the returned ledger and index. A producer schema newer than QSF supports is
/// refused rather than partially interpreted.
pub fn refresh_corpus(
    corpus_root: &Path,
    previous_ledger: Option<&CorpusLedger>,
) -> anyhow::Result<CorpusRefresh> {
    let marker = read_marker(corpus_root)?;
    let schema_drift = marker.schema_drift();
    if let CorpusSchemaDrift::Newer { found, supported } = schema_drift {
        bail!(
            "refusing corpus schema version {found}; QSF supports up to {supported} (structured drift: newer)"
        );
    }

    let previous_ledger = previous_ledger.filter(|ledger| ledger_matches_root(ledger, corpus_root));
    let article_files = enumerate_article_files(corpus_root, &marker)?;
    let mut content_hash_by_path = BTreeMap::new();
    let mut articles_by_content_hash = BTreeMap::new();
    let mut seen_paths = BTreeSet::new();
    let mut issues = Vec::new();
    let mut articles_added = 0;
    let mut articles_reused = 0;
    let mut articles_changed = 0;

    for path in &article_files {
        let relative_path = normalize_relative_path(corpus_root, path)?;
        seen_paths.insert(relative_path.clone());
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                issues.push(CorpusIngestIssue {
                    path: relative_path,
                    reason: format!("failed to read article: {error}"),
                });
                continue;
            }
        };
        let current_hash = content_hash(&source);
        let previous = previous_ledger.and_then(|ledger| ledger.article_for_path(&relative_path));
        if let Some(previous) = previous.filter(|article| article.content_hash == current_hash) {
            articles_reused += 1;
            content_hash_by_path.insert(relative_path, current_hash.clone());
            articles_by_content_hash.insert(current_hash, previous.clone());
            continue;
        }

        match parse_article(relative_path.clone(), &source) {
            Ok(article) => {
                if previous.is_some() {
                    articles_changed += 1;
                } else {
                    articles_added += 1;
                }
                content_hash_by_path.insert(relative_path, article.content_hash.clone());
                articles_by_content_hash.insert(article.content_hash.clone(), article);
            }
            Err(error) => issues.push(CorpusIngestIssue {
                path: relative_path,
                reason: error.to_string(),
            }),
        }
    }

    let articles_removed = previous_ledger
        .map(|articles| {
            articles
                .content_hash_by_path
                .keys()
                .filter(|path| !seen_paths.contains(*path))
                .count()
        })
        .unwrap_or(0);
    let index_started_at = Instant::now();
    let index = CorpusIndex::new(articles_by_content_hash.values().cloned().collect());
    let index_elapsed = index_started_at.elapsed();
    let ledger = CorpusLedger {
        ledger_version: INDEX_LEDGER_VERSION,
        corpus_root: root_identity(corpus_root),
        marker: marker.clone(),
        content_hash_by_path,
        articles_by_content_hash,
    };
    let report = CorpusIngestReport {
        corpus_root: root_identity(corpus_root),
        schema_version: marker.schema_version,
        producer: marker.producer,
        schema_drift,
        article_files_enumerated: article_files.len(),
        articles_added,
        articles_reused,
        articles_changed,
        articles_removed,
        articles_skipped: issues.len(),
        articles_indexed: index.article_count(),
        index_build_latency_ms: u64::try_from(index_elapsed.as_millis()).unwrap_or(u64::MAX),
        index_build_latency_ns: u64::try_from(index_elapsed.as_nanos()).unwrap_or(u64::MAX),
        issues,
    };

    Ok(CorpusRefresh {
        ledger,
        index,
        report,
    })
}

fn enumerate_article_files(
    corpus_root: &Path,
    marker: &CorpusMarker,
) -> anyhow::Result<Vec<PathBuf>> {
    let article_matcher = marker.article_matcher()?;
    let excluded_matcher = marker.excluded_matcher()?;
    let mut files = WalkDir::new(corpus_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let relative = normalize_relative_path(corpus_root, entry.path()).ok()?;
            (article_matcher.is_match(&relative) && !excluded_matcher.is_match(&relative))
                .then_some(entry.into_path())
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn normalize_relative_path(corpus_root: &Path, path: &Path) -> anyhow::Result<String> {
    let relative = path.strip_prefix(corpus_root).with_context(|| {
        format!(
            "article path {} escapes corpus root {}",
            path.display(),
            corpus_root.display()
        )
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn ledger_matches_root(ledger: &CorpusLedger, corpus_root: &Path) -> bool {
    ledger.corpus_root == root_identity(corpus_root)
}

fn root_identity(corpus_root: &Path) -> String {
    corpus_root
        .canonicalize()
        .unwrap_or_else(|_| corpus_root.to_path_buf())
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{CorpusLedger, load_ledger, refresh_corpus, write_ledger};
    use crate::{CorpusSchemaDrift, bundled_fixture_corpus_path};

    fn copy_fixture() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        copy_dir(bundled_fixture_corpus_path(), directory.path());
        directory
    }

    #[test]
    fn refresh_indexes_only_marker_selected_article_paths() {
        let directory = copy_fixture();

        let refreshed = refresh_corpus(directory.path(), None).unwrap();

        assert_eq!(refreshed.report.article_files_enumerated, 2);
        assert_eq!(refreshed.report.articles_indexed, 2);
        assert!(
            refreshed
                .ledger
                .content_hash_by_path
                .contains_key("linked/transition.md")
        );
        assert!(
            !refreshed
                .ledger
                .content_hash_by_path
                .contains_key("generated_artifacts/ignored.md")
        );
        assert!(
            !refreshed
                .ledger
                .content_hash_by_path
                .contains_key("internal_state/ignored.md")
        );
    }

    #[test]
    fn refresh_skips_malformed_articles_with_a_path_and_reason() {
        let directory = copy_fixture();
        fs::write(
            directory.path().join("bad.md"),
            "---\ntitle: Missing required values\n---\nBody",
        )
        .unwrap();
        fs::write(
            directory.path().join("harvester-corpus.json"),
            r#"{
  "schema_version": 1,
  "producer": "fixture-producer",
  "layout": {
    "articles": ["*.md", "linked/*.md"],
    "generated_artifacts": ["generated_artifacts"],
    "internal_state": ["internal_state"]
  }
}"#,
        )
        .unwrap();

        let refreshed = refresh_corpus(directory.path(), None).unwrap();

        assert_eq!(refreshed.report.articles_skipped, 1);
        assert_eq!(refreshed.report.issues[0].path, "bad.md");
        assert!(refreshed.report.issues[0].reason.contains("url"));
    }

    #[test]
    fn refresh_reuses_changes_adds_and_removes_by_content_hash() {
        let directory = copy_fixture();
        let first = refresh_corpus(directory.path(), None).unwrap();
        let original_hash = first.ledger.content_hash_by_path["ai-news.md"].clone();

        fs::write(
            directory.path().join("ai-news.md"),
            "---\ntitle: Changed AI infrastructure news\nurl: https://news.example.com/changed\nfetched_utc: 2026-07-10T12:00:00Z\n---\nChanged article body.",
        )
        .unwrap();
        fs::write(
            directory.path().join("new.md"),
            "---\ntitle: New model report\nurl: https://news.example.com/new\nfetched_utc: 2026-07-10T12:30:00Z\n---\nNew article body.",
        )
        .unwrap();
        fs::remove_file(directory.path().join("linked/transition.md")).unwrap();

        let second = refresh_corpus(directory.path(), Some(&first.ledger)).unwrap();

        assert_eq!(second.report.articles_changed, 1);
        assert_eq!(second.report.articles_added, 1);
        assert_eq!(second.report.articles_removed, 1);
        assert_ne!(
            second.ledger.content_hash_by_path["ai-news.md"],
            original_hash
        );
        assert_eq!(second.report.articles_reused, 0);
    }

    #[test]
    fn older_marker_schema_is_reported_but_ingested() {
        let directory = copy_fixture();
        let marker = directory.path().join("harvester-corpus.json");
        let raw = fs::read_to_string(&marker)
            .unwrap()
            .replace("\"schema_version\": 1", "\"schema_version\": 0");
        fs::write(marker, raw).unwrap();

        let refreshed = refresh_corpus(directory.path(), None).unwrap();

        assert!(matches!(
            refreshed.report.schema_drift,
            CorpusSchemaDrift::Older { .. }
        ));
    }

    #[test]
    fn ledger_round_trip_preserves_incremental_cache() {
        let directory = copy_fixture();
        let ledger_path = directory.path().join("state/world-corpus/index.json");
        let first = refresh_corpus(directory.path(), None).unwrap();
        write_ledger(&ledger_path, &first.ledger).unwrap();

        let loaded: CorpusLedger = load_ledger(&ledger_path).unwrap().unwrap();
        let second = refresh_corpus(directory.path(), Some(&loaded)).unwrap();

        assert_eq!(second.report.articles_reused, 2);
        assert_eq!(second.report.articles_added, 0);
        assert_eq!(second.report.articles_changed, 0);
    }

    fn copy_dir(source: impl AsRef<std::path::Path>, target: impl AsRef<std::path::Path>) {
        let source = source.as_ref();
        let target = target.as_ref();
        for entry in walkdir::WalkDir::new(source) {
            let entry = entry.unwrap();
            let relative = entry.path().strip_prefix(source).unwrap();
            let destination = target.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(destination).unwrap();
            } else {
                fs::copy(entry.path(), destination).unwrap();
            }
        }
    }
}
