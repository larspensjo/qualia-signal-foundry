use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use super::metadata::{kind_for_path, last_reviewed_for, maturity_for};
use super::{Allowlist, DocHit, MatchStrength};

const SNIPPET_BYTES: usize = 800;
const STOPWORDS: &[&str] = &[
    "about", "after", "again", "against", "are", "can", "could", "does", "for", "from", "how",
    "into", "its", "matching", "not", "the", "that", "this", "try", "what", "when", "where",
    "which", "with", "your",
];

struct MatchAnalysis {
    best_offset: usize,
    section_hint: Option<String>,
    heading_match: bool,
    occurrences: usize,
}

pub fn search(
    repo_root: &Path,
    allowlist: &Allowlist,
    query: &str,
    max_results: usize,
) -> Result<Vec<DocHit>> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let terms = query_terms(&needle);
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut scored = Vec::new();
    for entry in WalkDir::new(repo_root) {
        let entry = entry.with_context(|| format!("walk `{}`", repo_root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(repo_root)
            .with_context(|| format!("strip repo root `{}`", entry.path().display()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !allowlist.allows(&rel) {
            continue;
        }

        let body = fs::read_to_string(entry.path()).with_context(|| format!("read `{rel}`"))?;
        let Some(analysis) = analyze_matches(&body, &terms) else {
            continue;
        };
        let MatchAnalysis {
            best_offset,
            section_hint,
            heading_match,
            occurrences,
        } = analysis;

        let kind = kind_for_path(&rel);
        let maturity_tag = maturity_for(kind, &body);
        let last_reviewed = last_reviewed_for(&body);
        let snippet = extract_snippet(&body, best_offset, SNIPPET_BYTES);
        let match_strength = classify_strength(heading_match, occurrences);

        scored.push((
            heading_match,
            occurrences,
            DocHit {
                path: rel,
                kind,
                maturity_tag,
                last_reviewed,
                snippet,
                section_hint,
                match_strength,
            },
        ));
    }

    scored.sort_by(|left, right| {
        let (left_heading, left_occurrences, left_hit) = left;
        let (right_heading, right_occurrences, right_hit) = right;
        right_heading
            .cmp(left_heading)
            .then_with(|| right_occurrences.cmp(left_occurrences))
            .then_with(|| left_hit.path.cmp(&right_hit.path))
    });

    Ok(scored
        .into_iter()
        .take(max_results)
        .map(|(_, _, hit)| hit)
        .collect())
}

fn analyze_matches(body: &str, terms: &[String]) -> Option<MatchAnalysis> {
    let occurrences = count_occurrences(body, terms);
    if occurrences == 0 {
        return None;
    }

    let mut current_heading: Option<String> = None;
    let mut heading_best: Option<(usize, String)> = None;
    let mut body_best: Option<(usize, Option<String>)> = None;
    let mut offset = 0usize;

    for segment in body.split_inclusive('\n') {
        let line = segment.trim_end_matches('\n');
        let trimmed = line.trim_start();
        let is_heading = trimmed.starts_with("## ");
        if is_heading {
            current_heading = Some(trimmed.strip_prefix("## ").unwrap().trim().to_string());
        }

        if let Some(local_offset) = find_match_offset(line, terms) {
            let absolute_offset = offset + local_offset;
            if is_heading {
                match heading_best {
                    Some((best_offset, _)) if absolute_offset >= best_offset => {}
                    _ => {
                        heading_best =
                            Some((absolute_offset, current_heading.clone().unwrap_or_default()));
                    }
                }
            } else {
                match body_best {
                    Some((best_offset, _)) if absolute_offset >= best_offset => {}
                    _ => {
                        body_best = Some((absolute_offset, current_heading.clone()));
                    }
                }
            }
        }

        offset += segment.len();
    }

    if let Some((best_offset, heading)) = heading_best {
        return Some(MatchAnalysis {
            best_offset,
            section_hint: Some(heading),
            heading_match: true,
            occurrences,
        });
    }

    let (best_offset, section_hint) = body_best.expect("occurrences > 0 implies a body match");
    Some(MatchAnalysis {
        best_offset,
        section_hint,
        heading_match: false,
        occurrences,
    })
}

fn query_terms(needle: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for term in needle
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| term.len() >= 3 && !STOPWORDS.contains(term))
    {
        if !terms.iter().any(|existing| existing == term) {
            terms.push(term.to_string());
        }
    }
    terms
}

fn count_occurrences(body: &str, terms: &[String]) -> usize {
    let haystack = body.to_ascii_lowercase();
    terms
        .iter()
        .map(|term| haystack.matches(term).count())
        .sum()
}

fn find_match_offset(line: &str, terms: &[String]) -> Option<usize> {
    let haystack = line.to_ascii_lowercase();
    terms.iter().filter_map(|term| haystack.find(term)).min()
}

fn extract_snippet(body: &str, around: usize, byte_budget: usize) -> String {
    let bytes = body.as_bytes();
    let start = floor_char_boundary(body, around.saturating_sub(byte_budget / 2));
    let end = ceil_char_boundary(body, (around + byte_budget / 2).min(bytes.len()));
    body[start..end].to_string()
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn classify_strength(heading_match: bool, occurrences: usize) -> MatchStrength {
    if heading_match {
        MatchStrength::High
    } else if occurrences >= 3 {
        MatchStrength::Medium
    } else {
        MatchStrength::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::Allowlist;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn fixture_allowlist() -> Allowlist {
        Allowlist::from_file(fixtures_root().join("allowlist_basic.toml")).unwrap()
    }

    #[test]
    fn heading_match_outranks_body_match() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "Maturity", 6).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].section_hint.as_deref(), Some("Maturity"));
        assert_eq!(
            hits[0].match_strength,
            crate::project_docs::MatchStrength::High
        );
    }

    #[test]
    fn multi_term_query_matches_individual_terms() {
        let hits = search(
            &fixtures_root(),
            &fixture_allowlist(),
            "implementation status accepted",
            6,
        )
        .unwrap();

        assert!(!hits.is_empty());
        assert_eq!(hits[0].path, "sample_architecture.md");
        assert_eq!(
            hits[0].section_hint.as_deref(),
            Some("Implementation Status")
        );
    }

    #[test]
    fn stopword_only_query_returns_no_results() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "what are you", 6).unwrap();

        assert!(hits.is_empty());
    }

    #[test]
    fn heading_match_outranks_many_body_matches() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "maturity", 6).unwrap();
        let first_heading_rank = hits
            .iter()
            .position(|h| h.section_hint.as_deref() == Some("Maturity"))
            .expect("a heading match should exist");
        let body_heavy_rank = hits
            .iter()
            .position(|h| h.path.contains("sample_body_heavy"))
            .expect("body-heavy doc should appear");
        assert!(first_heading_rank < body_heavy_rank);
    }

    #[test]
    fn returns_unknown_kind_for_unstructured_doc() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "Unstructured", 6).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.kind == crate::project_docs::DocKind::Unknown)
        );
    }

    #[test]
    fn empty_results_when_no_match() {
        let hits = search(
            &fixtures_root(),
            &fixture_allowlist(),
            "xyzzyno-such-token",
            6,
        )
        .unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn empty_query_returns_no_results() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "   ", 6).unwrap();
        assert!(hits.is_empty());
    }
}
