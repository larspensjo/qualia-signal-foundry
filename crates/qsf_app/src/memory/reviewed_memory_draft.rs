use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use time::OffsetDateTime;

use crate::sleep::{SleepMemoryCandidate, SleepReport, parse_sleep_report};

use super::association::ensure_current_association_schema;
use super::fixtures::MemoryFixture;
use super::memory_record::{
    MEMORY_RECORD_SCHEMA_VERSION, MemoryRecord, MemoryRecordKind, ensure_current_memory_schema,
};

pub const REVIEWED_MEMORY_DRAFT_JSON: &str = "reviewed-memory-draft.json";
pub const REVIEWED_MEMORY_DRAFT_MARKDOWN: &str = "reviewed-memory-draft.md";
pub const DEFAULT_DRAFT_IMPORTANCE: f64 = 0.3;

#[derive(Clone, Debug, PartialEq)]
pub struct ReviewedMemoryDraft {
    pub source_sleep_run_id: String,
    pub source_sleep_report_path: PathBuf,
    pub fixture: MemoryFixture,
}

impl ReviewedMemoryDraft {
    pub fn record_count(&self) -> usize {
        self.fixture.records.len()
    }
}

pub fn load_reviewed_memory_draft(
    sleep_report_path: impl AsRef<Path>,
    created_at: OffsetDateTime,
) -> anyhow::Result<ReviewedMemoryDraft> {
    let sleep_report_path = sleep_report_path.as_ref();
    let contents = fs::read_to_string(sleep_report_path).with_context(|| {
        format!(
            "failed to read source sleep report `{}`",
            sleep_report_path.display()
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&contents).with_context(|| {
        format!(
            "failed to parse source sleep report JSON `{}`",
            sleep_report_path.display()
        )
    })?;
    let report = parse_sleep_report(&value).with_context(|| {
        format!(
            "failed to normalize source sleep report `{}`",
            sleep_report_path.display()
        )
    })?;

    Ok(convert_sleep_report_to_reviewed_memory_draft(
        &report,
        sleep_report_path,
        created_at,
    ))
}

pub fn convert_sleep_report_to_reviewed_memory_draft(
    report: &SleepReport,
    sleep_report_path: impl AsRef<Path>,
    created_at: OffsetDateTime,
) -> ReviewedMemoryDraft {
    let sleep_report_path = sleep_report_path.as_ref();
    let source_sleep_run_id = source_sleep_run_id_from_path(sleep_report_path);
    let sanitized_source_sleep_run_id = sanitize_memory_id_segment(&source_sleep_run_id);
    let records = report
        .memory_candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            convert_memory_candidate(
                candidate,
                &source_sleep_run_id,
                &sanitized_source_sleep_run_id,
                index,
                created_at,
            )
        })
        .collect();

    ReviewedMemoryDraft {
        source_sleep_run_id,
        source_sleep_report_path: sleep_report_path.to_path_buf(),
        fixture: MemoryFixture {
            records,
            associations: vec![],
        },
    }
}

pub fn write_reviewed_memory_draft(
    output_dir: impl AsRef<Path>,
    draft: &ReviewedMemoryDraft,
) -> anyhow::Result<()> {
    let output_dir = output_dir.as_ref();
    let json_path = output_dir.join(REVIEWED_MEMORY_DRAFT_JSON);
    let markdown_path = output_dir.join(REVIEWED_MEMORY_DRAFT_MARKDOWN);

    ensure_current_memory_schema(&draft.fixture.records)?;
    ensure_current_association_schema(&draft.fixture.associations)?;
    fs::write(&json_path, serde_json::to_string_pretty(&draft.fixture)?).with_context(|| {
        format!(
            "failed to write reviewed memory draft JSON `{}`",
            json_path.display()
        )
    })?;
    fs::write(
        &markdown_path,
        render_reviewed_memory_draft_markdown(draft, &json_path),
    )
    .with_context(|| {
        format!(
            "failed to write reviewed memory draft Markdown `{}`",
            markdown_path.display()
        )
    })?;

    Ok(())
}

pub fn render_reviewed_memory_draft_markdown(
    draft: &ReviewedMemoryDraft,
    draft_json_path: impl AsRef<Path>,
) -> String {
    let draft_json_path = draft_json_path.as_ref();
    let mut markdown = String::new();
    markdown.push_str("# Reviewed Memory Draft\n\n");
    markdown.push_str(&format!(
        "- Source sleep run: `{}`\n",
        draft.source_sleep_run_id
    ));
    markdown.push_str(&format!(
        "- Source sleep report: `{}`\n",
        draft.source_sleep_report_path.display()
    ));
    markdown.push_str(&format!("- Draft JSON: `{}`\n", draft_json_path.display()));
    markdown.push_str(
        "- Review policy: provisional until manually accepted; this file does not mutate durable memory\n",
    );
    markdown.push_str("- Associations: none generated in this draft\n");

    markdown.push_str("\n## Candidate Memory Records\n\n");

    if draft.fixture.records.is_empty() {
        markdown.push_str("- None recorded.\n");
    } else {
        for (index, record) in draft.fixture.records.iter().enumerate() {
            push_memory_record_markdown(&mut markdown, index, record);
        }
    }

    markdown.push_str("\n## File-Backed Voice Test\n\n");
    markdown.push_str(
        "After manual review, test this draft explicitly as a file-backed voice memory source:\n\n",
    );
    markdown.push_str("```powershell\n");
    markdown.push_str("$env:QSF_VOICE_MEMORY_SOURCE=\"file\"\n");
    markdown.push_str(&format!(
        "$env:QSF_VOICE_MEMORY_FILE=\"{}\"\n",
        powershell_path(draft_json_path)
    ));
    markdown.push_str("cargo run -p qsf_app -- experiment text-owned-voice-loop\n");
    markdown.push_str("```\n");

    markdown
}

fn push_memory_record_markdown(markdown: &mut String, index: usize, record: &MemoryRecord) {
    markdown.push_str(&format!(
        "### memory_candidates[{}] - {}\n\n",
        candidate_index_label(index),
        record.title
    ));
    markdown.push_str("Review:\n");
    markdown.push_str("- [ ] grounded\n");
    markdown.push_str("- [ ] summary\n");
    markdown.push_str("- [ ] source ref\n");
    markdown.push_str("- [ ] kind\n");
    markdown.push_str("- [ ] tags\n");
    markdown.push_str("- [ ] reject?\n\n");
    markdown.push_str(&format!("- Record id: `{}`\n", record.id));
    markdown.push_str(&format!("- Schema version: `{}`\n", record.schema_version));
    markdown.push_str(&format!(
        "- Kind: `{}`\n",
        memory_record_kind_label(&record.kind)
    ));
    markdown.push_str(&format!("- Importance: `{:.2}`\n", record.importance));
    markdown.push_str(&format!(
        "- Source reference: `{}`\n",
        record.source_reference
    ));
    markdown.push_str(&format!(
        "- Generated tags: {}\n",
        markdown_tag_list(&record.tags)
    ));
    markdown.push_str(&format!(
        "- Estimated tokens: `{}`\n",
        record.estimated_tokens
    ));
    markdown.push_str(&format!(
        "- Reinforcement count: `{}`\n",
        record.reinforcement_count
    ));
    markdown.push_str("\nSummary:\n\n");
    markdown.push_str("```text\n");
    markdown.push_str(&record.summary);
    if !record.summary.ends_with('\n') {
        markdown.push('\n');
    }
    markdown.push_str("```\n\n");
}

fn markdown_tag_list(tags: &[String]) -> String {
    if tags.is_empty() {
        "(none)".to_string()
    } else {
        tags.iter()
            .map(|tag| format!("`{tag}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn memory_record_kind_label(kind: &MemoryRecordKind) -> &'static str {
    match kind {
        MemoryRecordKind::Concept => "concept",
        MemoryRecordKind::ArchitectureNote => "architecture_note",
        MemoryRecordKind::Experiment => "experiment",
        MemoryRecordKind::Decision => "decision",
        MemoryRecordKind::Question => "question",
        MemoryRecordKind::Observation => "observation",
    }
}

fn powershell_path(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}

fn convert_memory_candidate(
    candidate: &SleepMemoryCandidate,
    source_sleep_run_id: &str,
    sanitized_source_sleep_run_id: &str,
    index: usize,
    created_at: OffsetDateTime,
) -> MemoryRecord {
    let index_label = candidate_index_label(index);
    // Memory ids use the sanitized run id segment. Source references keep the
    // original run directory name so reviewers can match the artifact path.
    let source_reference = candidate
        .source_reference
        .as_deref()
        .map(str::trim)
        .filter(|source_reference| !source_reference.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            format!("sleep-run:{source_sleep_run_id}#memory_candidates[{index_label}]")
        });
    let summary = candidate.summary.trim().to_string();

    MemoryRecord {
        schema_version: MEMORY_RECORD_SCHEMA_VERSION,
        id: format!("memory.sleep.{sanitized_source_sleep_run_id}.{index_label}"),
        kind: MemoryRecordKind::Observation,
        title: title_from_summary(&summary, index),
        summary: summary.clone(),
        tags: vec![],
        created_at,
        importance: candidate
            .importance
            .unwrap_or(DEFAULT_DRAFT_IMPORTANCE)
            .clamp(0.0, 1.0),
        reinforcement_count: 0,
        source_reference,
        estimated_tokens: estimated_tokens(&summary),
    }
}

fn source_sleep_run_id_from_path(path: &Path) -> String {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .filter(|name| !name.trim().is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "unknown-sleep-run".to_string())
}

fn sanitize_memory_id_segment(value: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_separator = false;

    for character in value.chars() {
        let next = if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            Some(character.to_ascii_lowercase())
        } else {
            Some('-')
        };

        if let Some(character) = next {
            if character == '-' {
                if !previous_was_separator && !sanitized.is_empty() {
                    sanitized.push(character);
                }
                previous_was_separator = true;
            } else {
                sanitized.push(character);
                previous_was_separator = false;
            }
        }
    }

    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        "unknown-sleep-run".to_string()
    } else {
        sanitized
    }
}

fn candidate_index_label(index: usize) -> String {
    format!("{:03}", index + 1)
}

fn title_from_summary(summary: &str, index: usize) -> String {
    let first_sentence = first_sentence(summary).trim();
    if first_sentence.is_empty() {
        return format!("Sleep memory candidate {}", candidate_index_label(index));
    }

    trim_to_word_boundary(first_sentence, 64)
}

fn first_sentence(summary: &str) -> &str {
    for (index, character) in summary.char_indices() {
        match character {
            '.' | '!' | '?' => return &summary[..=index],
            '\n' | '\r' => return &summary[..index],
            _ => {}
        }
    }

    summary
}

fn trim_to_word_boundary(value: &str, max_chars: usize) -> String {
    let mut end_byte = None;
    let mut last_word_boundary = None;

    for (char_count, (byte_index, character)) in value.char_indices().enumerate() {
        if char_count == max_chars {
            end_byte = Some(byte_index);
            break;
        }
        if character.is_whitespace() {
            last_word_boundary = Some(byte_index);
        }
    }

    if end_byte.is_none() {
        return value.to_string();
    }

    match last_word_boundary {
        Some(last_word_boundary) if last_word_boundary > 0 => {
            value[..last_word_boundary].trim_end().to_string()
        }
        _ => value.chars().take(max_chars).collect(),
    }
}

fn estimated_tokens(summary: &str) -> usize {
    let chars = summary.chars().count();
    usize::max(1, chars.div_ceil(4))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use time::format_description::well_known::Rfc3339;

    use crate::memory::MEMORY_RECORD_SCHEMA_VERSION;
    use crate::memory::association::ensure_current_association_schema;

    use super::*;

    #[test]
    fn converts_structured_sleep_memory_candidate() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": [
                {
                    "summary": "Reducers stay pure. Side effects return as actions.",
                    "importance": 0.82,
                    "source_reference": "runs/source/events.jsonl#L4"
                }
            ],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();

        let draft = convert_sleep_report_to_reviewed_memory_draft(
            &report,
            Path::new("runs/2026-05-12-101112-sleep-phase-session-summary/sleep-report.json"),
            timestamp(),
        );
        let record = &draft.fixture.records[0];

        assert_eq!(record.schema_version, MEMORY_RECORD_SCHEMA_VERSION);
        assert_eq!(
            record.id,
            "memory.sleep.2026-05-12-101112-sleep-phase-session-summary.001"
        );
        assert_eq!(record.kind, MemoryRecordKind::Observation);
        assert_eq!(record.title, "Reducers stay pure.");
        assert_eq!(
            record.summary,
            "Reducers stay pure. Side effects return as actions."
        );
        assert!(record.tags.is_empty());
        assert_eq!(record.created_at, timestamp());
        assert_eq!(record.importance, 0.82);
        assert_eq!(record.reinforcement_count, 0);
        assert_eq!(record.source_reference, "runs/source/events.jsonl#L4");
        assert_eq!(record.estimated_tokens, 13);
        assert!(draft.fixture.associations.is_empty());
        assert!(ensure_current_memory_schema(&draft.fixture.records).is_ok());
    }

    #[test]
    fn converts_string_only_candidate_with_defaults() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": ["Remember that sleep output remains provisional"],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();

        let draft = convert_sleep_report_to_reviewed_memory_draft(
            &report,
            Path::new("runs/Sleep Run With Spaces/sleep-report.json"),
            timestamp(),
        );
        let record = &draft.fixture.records[0];

        assert_eq!(record.id, "memory.sleep.sleep-run-with-spaces.001");
        assert_eq!(record.importance, DEFAULT_DRAFT_IMPORTANCE);
        assert_eq!(
            record.source_reference,
            "sleep-run:Sleep Run With Spaces#memory_candidates[001]"
        );
        assert_eq!(
            record.title,
            "Remember that sleep output remains provisional"
        );
        assert_eq!(record.estimated_tokens, 12);
    }

    #[test]
    fn empty_candidates_produce_valid_empty_fixture_with_current_schemas() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": [],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();

        let draft = convert_sleep_report_to_reviewed_memory_draft(
            &report,
            Path::new("runs/source-sleep/sleep-report.json"),
            timestamp(),
        );

        assert!(draft.fixture.records.is_empty());
        assert!(draft.fixture.associations.is_empty());
        assert!(ensure_current_memory_schema(&draft.fixture.records).is_ok());
        assert!(ensure_current_association_schema(&draft.fixture.associations).is_ok());
    }

    #[test]
    fn candidate_importance_is_clamped_when_built_directly() {
        let high_importance = SleepMemoryCandidate {
            summary: "High importance should clamp.".to_string(),
            importance: Some(1.4),
            source_reference: None,
        };
        let low_importance = SleepMemoryCandidate {
            summary: "Low importance should clamp.".to_string(),
            importance: Some(-0.2),
            source_reference: None,
        };

        let high_record =
            convert_memory_candidate(&high_importance, "source-run", "source-run", 0, timestamp());
        let low_record =
            convert_memory_candidate(&low_importance, "source-run", "source-run", 1, timestamp());

        assert_eq!(high_record.importance, 1.0);
        assert_eq!(low_record.importance, 0.0);
    }

    #[test]
    fn repeated_conversion_produces_same_ids() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": ["First memory.", "Second memory."],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();
        let path = Path::new("runs/repeated-source/sleep-report.json");

        let first = convert_sleep_report_to_reviewed_memory_draft(
            &report,
            path,
            timestamp_from("2026-05-12T10:11:12Z"),
        );
        let second = convert_sleep_report_to_reviewed_memory_draft(
            &report,
            path,
            timestamp_from("2026-05-12T10:12:12Z"),
        );
        let first_ids: Vec<_> = first
            .fixture
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect();
        let second_ids: Vec<_> = second
            .fixture
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect();

        assert_eq!(first_ids, second_ids);
    }

    fn sample_reviewed_memory_draft() -> ReviewedMemoryDraft {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": [
                {
                    "summary": "First memory should be inspected before use.",
                    "importance": 0.71,
                    "source_reference": "events.jsonl#first"
                },
                "Second memory."
            ],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();

        convert_sleep_report_to_reviewed_memory_draft(
            &report,
            Path::new("runs/source-sleep-run/sleep-report.json"),
            timestamp(),
        )
    }

    fn render_sample_review_markdown() -> String {
        render_reviewed_memory_draft_markdown(
            &sample_reviewed_memory_draft(),
            Path::new("runs/conversion-run/reviewed-memory-draft.json"),
        )
    }

    #[test]
    fn markdown_includes_review_policy_and_sources() {
        let markdown = render_sample_review_markdown();

        assert!(markdown.contains("Source sleep run: `source-sleep-run`"));
        assert!(
            markdown.contains("Source sleep report: `runs/source-sleep-run/sleep-report.json`")
        );
        assert!(markdown.contains("Review policy: provisional until manually accepted"));
        assert!(markdown.contains("## Candidate Memory Records"));
    }

    #[test]
    fn markdown_includes_per_record_details_and_checklist() {
        let markdown = render_sample_review_markdown();

        assert!(markdown.contains("memory_candidates[001]"));
        assert!(markdown.contains("memory_candidates[002]"));
        assert!(markdown.contains("Record id: `memory.sleep.source-sleep-run.001`"));
        assert!(markdown.contains("Kind: `observation`"));
        assert!(markdown.contains("Importance: `0.71`"));
        assert!(markdown.contains("Source reference: `events.jsonl#first`"));
        assert!(markdown.contains("Generated tags: (none)"));
        assert!(markdown.contains("First memory should be inspected before use."));
        assert!(markdown.contains("- [ ] grounded"));
        assert!(markdown.contains("- [ ] summary"));
        assert!(markdown.contains("- [ ] source ref"));
        assert!(markdown.contains("- [ ] kind"));
        assert!(markdown.contains("- [ ] tags"));
        assert!(markdown.contains("- [ ] reject?"));
    }

    #[test]
    fn markdown_includes_voice_test_command() {
        let markdown = render_sample_review_markdown();

        assert!(markdown.contains("$env:QSF_VOICE_MEMORY_SOURCE=\"file\""));
        assert!(markdown.contains(
            "$env:QSF_VOICE_MEMORY_FILE=\"runs\\conversion-run\\reviewed-memory-draft.json\""
        ));
        assert!(markdown.contains("cargo run -p qsf_app -- experiment text-owned-voice-loop"));
    }

    #[test]
    fn empty_summary_uses_fallback_title() {
        let candidate = SleepMemoryCandidate {
            summary: "   ".to_string(),
            importance: None,
            source_reference: Some(" ".to_string()),
        };

        let record =
            convert_memory_candidate(&candidate, "source-run", "source-run", 4, timestamp());

        assert_eq!(record.title, "Sleep memory candidate 005");
        assert_eq!(
            record.source_reference,
            "sleep-run:source-run#memory_candidates[005]"
        );
        assert_eq!(record.estimated_tokens, 1);
    }

    #[test]
    fn title_stops_at_newline_before_markdown_rendering() {
        let candidate = SleepMemoryCandidate {
            summary: "First line title\nSecond line details.".to_string(),
            importance: None,
            source_reference: None,
        };

        let record =
            convert_memory_candidate(&candidate, "source-run", "source-run", 0, timestamp());

        assert_eq!(record.title, "First line title");
        assert!(!record.title.contains('\n'));
    }

    fn timestamp() -> OffsetDateTime {
        timestamp_from("2026-05-12T10:11:12Z")
    }

    fn timestamp_from(value: &str) -> OffsetDateTime {
        OffsetDateTime::parse(value, &Rfc3339).unwrap()
    }
}
