use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;

use crate::DiagnosticRecord;

#[derive(Clone)]
pub struct DiagnosticWriter {
    path: PathBuf,
    file: std::sync::Arc<Mutex<BufWriter<File>>>,
}

impl DiagnosticWriter {
    pub fn create(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create diagnostics dir `{}`", parent.display())
            })?;
        }
        let file = File::options()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open diagnostics log `{}`", path.display()))?;
        Ok(Self {
            path,
            file: std::sync::Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, record: &DiagnosticRecord) -> anyhow::Result<()> {
        let mut guard = self.file.lock().expect("diagnostic writer mutex poisoned");
        serde_json::to_writer(&mut *guard, record)
            .context("failed to serialize diagnostic record")?;
        guard
            .write_all(b"\n")
            .context("failed to append newline to diagnostic record")?;
        guard.flush().context("failed to flush diagnostic record")?;
        Ok(())
    }
}
