use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

use qsf_diagnostics::{DiagnosticRecord, decode_envelope};

use crate::transcript::join::LedgerEntry;
use crate::transcript::model::SkippedLineView;

/// Resolves which ledger to read. An explicit session names its file directly; otherwise the newest
/// `*.jsonl` wins, so a `-RandomSessionId` run is found without looking up its UUID. This mirrors
/// how `goals` auto-selects a continuity session
/// (`crate::goal_detail_loading::resolve_session`) over a different directory.
///
/// Ties on modification time break on file name, descending. `read_dir` order is unspecified, so
/// without an explicit tie-break two ledgers sharing a timestamp could resolve differently between
/// runs or filesystems — and since this is the default invocation, that would silently show a
/// different conversation than the last one.
pub fn resolve_ledger_path(state_dir: &Path, session: Option<&str>) -> anyhow::Result<PathBuf> {
    let diagnostics_dir = state_dir.join("diagnostics");
    if let Some(session_id) = session {
        let path = diagnostics_dir.join(format!("{session_id}.jsonl"));
        if !path.exists() {
            anyhow::bail!(
                "no diagnostics ledger for session `{session_id}` at `{}`",
                path.display()
            );
        }
        return Ok(path);
    }

    // Collect, then pick by an explicit total order. Sorting the candidates rather than tracking a
    // running maximum makes the tie-break obvious and keeps `read_dir` order out of the result.
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let entries = fs::read_dir(&diagnostics_dir).with_context(|| {
        format!(
            "failed to read diagnostics directory `{}`",
            diagnostics_dir.display()
        )
    })?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        if !entry.metadata()?.is_file() {
            continue;
        }
        candidates.push((entry.metadata()?.modified()?, path));
    }

    candidates.sort_by(|(left_time, left_path), (right_time, right_path)| {
        left_time
            .cmp(right_time)
            .then_with(|| left_path.file_name().cmp(&right_path.file_name()))
    });

    candidates.pop().map(|(_, path)| path).ok_or_else(|| {
        anyhow::anyhow!(
            "no diagnostics ledger found under `{}`",
            diagnostics_dir.display()
        )
    })
}

/// Reads every non-blank line into a `LedgerEntry`, preserving file order.
///
/// A line this build cannot deserialize does not abort the read: the ledger is append-only and
/// outlives builds, so old runs may hold record shapes this build no longer knows. Instead the line
/// is recorded as skipped, located by line number and — when the envelope decodes — by kind and
/// exchange index, so the emitted artifact can say which turn lost which section.
pub fn load_ledger(path: &Path) -> anyhow::Result<Vec<LedgerEntry>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read diagnostics ledger `{}`", path.display()))?;
    let mut entries = Vec::new();

    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<DiagnosticRecord>(line) {
            Ok(record) => entries.push(LedgerEntry::Record(Box::new(record))),
            Err(error) => {
                let envelope = decode_envelope(line);
                entries.push(LedgerEntry::Skipped(SkippedLineView {
                    line_number: index + 1,
                    kind: envelope.as_ref().and_then(|e| e.kind.clone()),
                    exchange_index: envelope.as_ref().and_then(|e| e.exchange_index),
                    error: error.to_string(),
                }));
            }
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn an_explicit_session_selects_its_own_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        fs::write(diagnostics.join("default.jsonl"), "").expect("write");
        fs::write(diagnostics.join("run-2.jsonl"), "").expect("write");

        let resolved = resolve_ledger_path(dir.path(), Some("run-2")).expect("resolve");

        assert_eq!(resolved, diagnostics.join("run-2.jsonl"));
    }

    #[test]
    fn an_absent_session_is_an_error_naming_the_expected_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("diagnostics")).expect("create");

        let error =
            resolve_ledger_path(dir.path(), Some("missing")).expect_err("absent ledger must fail");

        assert!(error.to_string().contains("missing.jsonl"));
    }

    #[test]
    fn unparseable_lines_are_skipped_and_located_rather_than_aborting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ledger.jsonl");
        fs::write(
            &path,
            "{\"kind\":\"session_allocated\",\"qsf_session_id\":\"s\",\"at\":\"1970-01-01T00:00:00Z\"}\n\
             {\"kind\":\"from_a_future_build\",\"qsf_session_id\":\"s\",\"exchange_index\":2}\n",
        )
        .expect("write");

        let entries = load_ledger(&path).expect("load");

        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], LedgerEntry::Record(_)));
        let LedgerEntry::Skipped(skipped) = &entries[1] else {
            panic!("second line must be skipped");
        };
        assert_eq!(skipped.line_number, 2);
        assert_eq!(skipped.kind.as_deref(), Some("from_a_future_build"));
        assert_eq!(
            skipped.exchange_index,
            Some(2),
            "the envelope's exchange index must survive so the turn can be marked incomplete"
        );
    }

    #[test]
    fn a_line_that_is_not_json_is_skipped_with_no_kind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ledger.jsonl");
        fs::write(&path, "this is not json\n").expect("write");

        let entries = load_ledger(&path).expect("load");

        let LedgerEntry::Skipped(skipped) = &entries[0] else {
            panic!("must be skipped");
        };
        assert_eq!(skipped.kind, None);
        assert_eq!(skipped.exchange_index, None);
    }

    #[test]
    fn the_newest_ledger_wins_automatic_selection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        let older = diagnostics.join("older.jsonl");
        let newer = diagnostics.join("newer.jsonl");
        fs::write(&older, "").expect("write");
        fs::write(&newer, "").expect("write");
        set_modified(&older, 1_000);
        set_modified(&newer, 2_000);

        let resolved = resolve_ledger_path(dir.path(), None).expect("resolve");

        assert_eq!(resolved, newer);
    }

    #[test]
    fn equal_timestamps_are_broken_by_file_name_not_directory_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        for name in ["aaa.jsonl", "zzz.jsonl", "mmm.jsonl"] {
            let path = diagnostics.join(name);
            fs::write(&path, "").expect("write");
            set_modified(&path, 5_000);
        }

        let resolved = resolve_ledger_path(dir.path(), None).expect("resolve");

        assert_eq!(
            resolved,
            diagnostics.join("zzz.jsonl"),
            "the greatest file name wins a timestamp tie"
        );
    }

    #[test]
    fn non_jsonl_files_are_ignored_by_automatic_selection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        let ledger = diagnostics.join("default.jsonl");
        fs::write(&ledger, "").expect("write");
        set_modified(&ledger, 1_000);
        let note = diagnostics.join("notes.txt");
        fs::write(&note, "").expect("write");
        set_modified(&note, 9_000);

        let resolved = resolve_ledger_path(dir.path(), None).expect("resolve");

        assert_eq!(resolved, ledger);
    }

    /// Sets a file's modification time to a fixed offset from the Unix epoch so tie-break behavior
    /// is testable rather than dependent on write-order timing. Uses `File::set_modified`, stable
    /// since Rust 1.75, so this needs no new dependency.
    fn set_modified(path: &std::path::Path, epoch_secs: u64) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open for mtime");
        file.set_modified(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(epoch_secs),
        )
        .expect("set mtime");
    }
}
