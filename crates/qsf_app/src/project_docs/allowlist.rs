use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Allowlist {
    include: GlobSet,
    exclude: GlobSet,
}

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

impl Allowlist {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("read allowlist `{}`", path.display()))?;
        Self::from_str(&raw)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(raw: &str) -> Result<Self> {
        let parsed: AllowlistFile = toml::from_str(raw).context("parse allowlist toml")?;
        Ok(Self {
            include: build_globset(&parsed.include)?,
            exclude: build_globset(&parsed.exclude)?,
        })
    }

    /// Evaluates a clean, repo-relative, forward-slash path.
    pub fn allows(&self, repo_relative_path: &str) -> bool {
        if self.exclude.is_match(repo_relative_path) {
            return false;
        }
        self.include.is_match(repo_relative_path)
    }
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).with_context(|| format!("invalid glob `{pattern}`"))?);
    }
    builder.build().context("compile glob set")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// `config/project-doc-introspection.toml` lives at the workspace root,
    /// two levels above this crate's manifest dir.
    fn workspace_config_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../config/project-doc-introspection.toml")
    }

    #[test]
    fn accepts_path_matching_include_only() {
        let allowlist = Allowlist::from_str(
            r#"include=["docs/**/*.md"]
exclude=[]"#,
        )
        .unwrap();
        assert!(allowlist.allows("docs/ProjectFrame/ProjectVision.md"));
    }

    #[test]
    fn rejects_path_outside_include() {
        let allowlist = Allowlist::from_str(
            r#"include=["docs/**/*.md"]
exclude=[]"#,
        )
        .unwrap();
        assert!(!allowlist.allows("crates/qsf_app/src/main.rs"));
    }

    #[test]
    fn exclude_overrides_include() {
        let allowlist = Allowlist::from_str(
            r#"include=["docs/**/*.md"]
exclude=["docs/Reviews/**"]"#,
        )
        .unwrap();
        assert!(!allowlist.allows("docs/Reviews/Review.X.md"));
        assert!(allowlist.allows("docs/Architecture/Architecture.Overview.md"));
    }

    #[test]
    fn default_production_allowlist_excludes_reviews() {
        let allowlist =
            Allowlist::from_file(workspace_config_path()).expect("production allowlist must load");
        assert!(!allowlist.allows("docs/Reviews/anything.md"));
        assert!(allowlist.allows("docs/ProjectFrame/ProjectVision.md"));
        assert!(allowlist.allows("docs/DecisionLog.md"));
    }
}
