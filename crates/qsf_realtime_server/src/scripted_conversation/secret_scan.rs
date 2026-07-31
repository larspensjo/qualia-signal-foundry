use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretScanReport {
    pub found: bool,
    pub paths: Vec<String>,
}

pub fn scan_for_secret(bytes: &[u8], secret: &str) -> bool {
    !secret.is_empty()
        && bytes
            .windows(secret.len())
            .any(|candidate| candidate == secret.as_bytes())
}

pub fn scan_run_dir(path: &Path, secret: &str) -> anyhow::Result<SecretScanReport> {
    let mut report = SecretScanReport::default();
    scan_path(path, path, secret, &mut report)?;
    Ok(report)
}

fn scan_path(
    root: &Path,
    path: &Path,
    secret: &str,
    report: &mut SecretScanReport,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(path)? {
        let child = entry?.path();
        if child.is_dir() {
            scan_path(root, &child, secret, report)?;
        } else if scan_for_secret(&fs::read(&child)?, secret) {
            report.found = true;
            report.paths.push(
                child
                    .strip_prefix(root)
                    .unwrap_or(&child)
                    .display()
                    .to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::scan_for_secret;
    #[test]
    fn detects_key_but_not_hash() {
        assert!(scan_for_secret(b"x sk-test-secret y", "sk-test-secret"));
        assert!(!scan_for_secret(b"sha256:08b932", "sk-test-secret"));
    }
}
