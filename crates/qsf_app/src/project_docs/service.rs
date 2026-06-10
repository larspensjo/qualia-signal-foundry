use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{Allowlist, DocHit, DocRead, read, search};

#[derive(Clone)]
pub struct ProjectDocService {
    repo_root: PathBuf,
    allowlist_path: PathBuf,
}

impl ProjectDocService {
    pub fn new(repo_root: impl Into<PathBuf>, allowlist_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            allowlist_path: allowlist_path.into(),
        }
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn allowlist(&self) -> Result<Allowlist> {
        Allowlist::from_file(&self.allowlist_path)
    }

    pub fn search(&self, query: &str, max_results: usize) -> Result<Vec<DocHit>> {
        let allowlist = self.allowlist()?;
        search(&self.repo_root, &allowlist, query, max_results)
    }

    pub fn read(
        &self,
        relative_path: &str,
        focus: Option<&str>,
        max_tokens: usize,
    ) -> Result<DocRead> {
        let allowlist = self.allowlist()?;
        read(
            &self.repo_root,
            &allowlist,
            relative_path,
            focus,
            max_tokens,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn repo_root_for_tests() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    #[test]
    fn hot_reloads_allowlist_between_calls() {
        let tmp = TempDir::new().unwrap();
        let config = tmp.path().join("allowlist.toml");
        std::fs::write(
            &config,
            r#"include=["sample_concept.md"]
exclude=[]"#,
        )
        .unwrap();

        let service = ProjectDocService::new(repo_root_for_tests(), config.clone());

        assert!(service.allowlist().unwrap().allows("sample_concept.md"));

        std::fs::write(
            &config,
            r#"include=[]
exclude=[]"#,
        )
        .unwrap();
        assert!(!service.allowlist().unwrap().allows("sample_concept.md"));
    }
}
