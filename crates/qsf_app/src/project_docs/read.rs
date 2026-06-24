use std::fs;
use std::path::{Component, Path};

use anyhow::{Context, Result, bail};

use super::metadata::{kind_for_path, last_reviewed_for, maturity_for};
use super::{Allowlist, DocRead};

#[derive(Clone, Debug)]
struct Section {
    heading: String,
    text: String,
}

impl Section {
    fn matches_focus(&self, needle: &str) -> bool {
        self.heading.to_ascii_lowercase().contains(needle)
            || self.text.to_ascii_lowercase().contains(needle)
    }
}

pub fn read(
    repo_root: &Path,
    allowlist: &Allowlist,
    relative_path: &str,
    focus: Option<&str>,
    max_tokens: usize,
) -> Result<DocRead> {
    let normalized = normalize_repo_relative(relative_path)?;
    if !allowlist.allows(&normalized) {
        bail!("path `{normalized}` not in allowlist");
    }

    let body = fs::read_to_string(repo_root.join(&normalized))
        .with_context(|| format!("read `{normalized}`"))?;
    let kind = kind_for_path(&normalized);
    let maturity_tag = maturity_for(kind, &body);
    let last_reviewed = last_reviewed_for(&body);

    let (preamble, sections) = split_sections(&body);
    let byte_budget = max_tokens.saturating_mul(4);
    let focus_needle = focus.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_ascii_lowercase())
        }
    });

    let mut content = String::new();
    content.push_str(&preamble);

    let mut omitted_sections = Vec::new();
    let mut included_status = false;
    let mut included_implementation_status = false;

    for section in sections {
        match section.heading.as_str() {
            "Status" => {
                if !included_status {
                    content.push_str(&section.text);
                    included_status = true;
                } else {
                    omitted_sections.push(section.heading);
                }
                continue;
            }
            "Implementation Status" => {
                if !included_implementation_status {
                    content.push_str(&section.text);
                    included_implementation_status = true;
                } else {
                    omitted_sections.push(section.heading);
                }
                continue;
            }
            _ => {}
        }

        let matches_focus = focus_needle
            .as_ref()
            .map(|needle| section.matches_focus(needle))
            .unwrap_or(true);

        if matches_focus && content.len() + section.text.len() <= byte_budget {
            content.push_str(&section.text);
        } else {
            omitted_sections.push(section.heading);
        }
    }

    Ok(DocRead {
        path: normalized,
        kind,
        maturity_tag,
        last_reviewed,
        content,
        is_full: omitted_sections.is_empty(),
        omitted_sections,
    })
}

fn normalize_repo_relative(path: &str) -> Result<String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        bail!("path `{path}` must be repo-relative");
    }

    let mut parts = Vec::new();
    for component in candidate.components() {
        match component {
            Component::Normal(c) => parts.push(
                c.to_str()
                    .context("non-utf8 path component in normalized path")?,
            ),
            Component::CurDir => {}
            Component::ParentDir => bail!("path `{path}` must not contain `..`"),
            Component::Prefix(_) | Component::RootDir => {
                bail!("path `{path}` must be repo-relative")
            }
        }
    }

    if parts.is_empty() {
        bail!("path `{path}` must name a document");
    }

    Ok(parts.join("/"))
}

fn split_sections(body: &str) -> (String, Vec<Section>) {
    let mut preamble = String::new();
    let mut sections = Vec::new();
    let mut seen_section = false;

    for segment in body.split_inclusive('\n') {
        let trimmed = segment.trim_start();
        if trimmed.starts_with("## ") {
            seen_section = true;
            sections.push(Section {
                heading: trimmed.strip_prefix("## ").unwrap().trim().to_string(),
                text: String::new(),
            });
        }

        if seen_section {
            if let Some(last) = sections.last_mut() {
                last.text.push_str(segment);
            }
        } else {
            preamble.push_str(segment);
        }
    }

    (preamble, sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{Allowlist, DocKind, MaturityTag};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn allow_all() -> Allowlist {
        Allowlist::from_str(
            r#"include=["**/*.md"]
exclude=[]"#,
        )
        .unwrap()
    }

    #[test]
    fn reads_whole_doc_when_under_budget() {
        let doc = read(
            &fixtures_root(),
            &allow_all(),
            "sample_concept.md",
            None,
            10_000,
        )
        .unwrap();
        assert!(doc.is_full);
        assert!(doc.omitted_sections.is_empty());
        assert_eq!(doc.kind, DocKind::Unknown);
        assert_eq!(doc.maturity_tag, MaturityTag::NotApplicable);
    }

    #[test]
    fn focused_read_returns_named_section_plus_head() {
        let doc = read(
            &fixtures_root(),
            &allow_all(),
            "sample_architecture.md",
            Some("Implementation Status"),
            10_000,
        )
        .unwrap();
        assert!(doc.content.contains("Implementation Status"));
        assert!(doc.content.contains("Last reviewed"));
    }

    #[test]
    fn head_section_is_not_duplicated() {
        let doc = read(
            &fixtures_root(),
            &allow_all(),
            "sample_architecture.md",
            None,
            10_000,
        )
        .unwrap();
        assert_eq!(doc.content.matches("## Implementation Status").count(), 1);
    }

    #[test]
    fn refuses_path_outside_allowlist() {
        let allow_none = Allowlist::from_str(
            r#"include=[]
exclude=[]"#,
        )
        .unwrap();
        let err = read(
            &fixtures_root(),
            &allow_none,
            "sample_concept.md",
            None,
            10_000,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not in allowlist"));
    }

    #[test]
    fn refuses_parent_directory_traversal() {
        let err = read(
            &fixtures_root(),
            &allow_all(),
            "../../README.md",
            None,
            10_000,
        )
        .unwrap_err();
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn refuses_absolute_path() {
        let abs = if cfg!(windows) {
            r"C:\Windows\system.ini"
        } else {
            "/etc/passwd"
        };
        let err = read(&fixtures_root(), &allow_all(), abs, None, 10_000).unwrap_err();
        assert!(err.to_string().contains("repo-relative"));
    }

    #[test]
    fn omitted_sections_populated_when_truncated() {
        let doc = read(
            &fixtures_root(),
            &allow_all(),
            "sample_architecture.md",
            None,
            8,
        )
        .unwrap();
        assert!(!doc.is_full);
        assert!(!doc.omitted_sections.is_empty());
    }
}
