use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::observability::event_log::{EventRecord, EventType};
use crate::observability::trace::TraceRecord;

use super::influence::reply_overlaps_excerpt;

const PROJECT_DOC_SEARCH_OPERATION: &str = "project_doc_search";
const PROJECT_DOC_READ_OPERATION: &str = "project_doc_read";
const PROJECT_DOC_INFLUENCE_OPERATION: &str = "project_doc_influence";

pub fn enrich(run_dir: &Path) -> Result<usize> {
    let events_path = run_dir.join("events.jsonl");
    let traces_path = run_dir.join("traces.jsonl");

    let turn_replies = load_turn_completed_replies(&events_path)?;
    let trace_records: Vec<TraceRecord> = read_jsonl(&traces_path)?;
    let mut already_enriched = already_enriched_source_trace_ids(&trace_records)?;

    let mut appended_records = Vec::new();

    for trace in &trace_records {
        if !is_project_doc_trace(trace) || details_refused(&trace.details) {
            continue;
        }

        let source_trace_id = trace.trace_id;
        if already_enriched.contains(&source_trace_id) {
            continue;
        }

        let turn_index = details_turn_index(&trace.details)
            .with_context(|| format!("project-doc trace `{source_trace_id}` missing turn_index"))?;
        let Some(reply) = turn_replies.get(&turn_index) else {
            continue;
        };

        let excerpt = project_doc_trace_excerpt(trace)
            .with_context(|| format!("project-doc trace `{source_trace_id}` missing content"))?;
        let influenced_reply = reply_overlaps_excerpt(reply, &excerpt);

        appended_records.push(build_influence_record(trace, turn_index, influenced_reply));
        already_enriched.insert(source_trace_id);
    }

    if appended_records.is_empty() {
        return Ok(0);
    }

    append_trace_records(&traces_path, &appended_records)?;
    Ok(appended_records.len())
}

fn load_turn_completed_replies(events_path: &Path) -> Result<HashMap<usize, String>> {
    let mut replies = HashMap::new();

    for event in read_jsonl::<EventRecord>(events_path)? {
        if event.event_type != EventType::TurnCompleted {
            continue;
        }

        let payload: TurnCompletedEventPayload = serde_json::from_value(event.payload)
            .context("failed to parse TurnCompleted event payload")?;
        if replies
            .insert(payload.turn.index, payload.turn.assistant_response)
            .is_some()
        {
            bail!(
                "duplicate TurnCompleted event for turn index {}",
                payload.turn.index
            );
        }
    }

    Ok(replies)
}

fn already_enriched_source_trace_ids(trace_records: &[TraceRecord]) -> Result<HashSet<Uuid>> {
    let mut already_enriched = HashSet::new();

    for trace in trace_records {
        if trace.operation != PROJECT_DOC_INFLUENCE_OPERATION {
            continue;
        }

        let details = trace
            .details
            .as_object()
            .context("project_doc_influence trace details must be an object")?;
        let source_trace_id = details
            .get("source_trace_id")
            .and_then(Value::as_str)
            .context("project_doc_influence trace missing source_trace_id")?;
        let source_trace_id = Uuid::parse_str(source_trace_id)
            .context("project_doc_influence source_trace_id is not a UUID")?;
        already_enriched.insert(source_trace_id);
    }

    Ok(already_enriched)
}

fn is_project_doc_trace(trace: &TraceRecord) -> bool {
    matches!(
        trace.operation.as_str(),
        PROJECT_DOC_SEARCH_OPERATION | PROJECT_DOC_READ_OPERATION
    )
}

fn details_refused(details: &Value) -> bool {
    details
        .get("refused")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn details_turn_index(details: &Value) -> Result<usize> {
    let turn_index = details
        .get("turn_index")
        .and_then(Value::as_u64)
        .context("missing turn_index")?;
    usize::try_from(turn_index).context("turn_index does not fit in usize")
}

fn project_doc_trace_excerpt(trace: &TraceRecord) -> Result<String> {
    match trace.operation.as_str() {
        PROJECT_DOC_SEARCH_OPERATION => {
            let details = trace
                .details
                .as_object()
                .context("project_doc_search trace details must be an object")?;
            let hits = details
                .get("hits")
                .and_then(Value::as_array)
                .context("project_doc_search trace missing hits array")?;
            let mut excerpt = String::new();

            for hit in hits {
                let hit = hit
                    .as_object()
                    .context("project_doc_search hit must be an object")?;
                if let Some(snippet) = hit.get("snippet").and_then(Value::as_str) {
                    push_fragment(&mut excerpt, snippet);
                } else {
                    bail!("project_doc_search hit missing snippet");
                }
                if let Some(section_hint) = hit.get("section_hint").and_then(Value::as_str) {
                    push_fragment(&mut excerpt, section_hint);
                }
            }

            Ok(excerpt)
        }
        PROJECT_DOC_READ_OPERATION => {
            let details = trace
                .details
                .as_object()
                .context("project_doc_read trace details must be an object")?;
            let read = details
                .get("read")
                .and_then(Value::as_object)
                .context("project_doc_read trace missing read object")?;
            let content = read
                .get("content")
                .and_then(Value::as_str)
                .context("project_doc_read trace missing read.content")?;
            Ok(content.to_string())
        }
        _ => bail!("unsupported project-doc trace operation"),
    }
}

fn push_fragment(target: &mut String, fragment: &str) {
    if !target.is_empty() {
        target.push(' ');
    }
    target.push_str(fragment);
}

fn build_influence_record(
    source_trace: &TraceRecord,
    turn_index: usize,
    influenced_reply: bool,
) -> TraceRecord {
    TraceRecord::new(
        source_trace.experiment_id.clone(),
        PROJECT_DOC_INFLUENCE_OPERATION,
        format!("source_trace_id={}", source_trace.trace_id),
        format!("influenced_reply={influenced_reply}"),
    )
    .with_details(json!({
        "source_trace_id": source_trace.trace_id,
        "source_operation": source_trace.operation.clone(),
        "turn_index": turn_index,
        "influenced_reply": influenced_reply,
    }))
}

fn append_trace_records(path: &Path, records: &[TraceRecord]) -> Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open `{}` for append", path.display()))?;
    let mut writer = BufWriter::new(file);

    for record in records {
        serde_json::to_writer(&mut writer, record)
            .context("failed to serialize project-doc influence trace")?;
        writer
            .write_all(b"\n")
            .context("failed to write project-doc influence trace newline")?;
    }

    writer.flush().context("failed to flush appended traces")?;
    Ok(())
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let file = File::open(path)
        .with_context(|| format!("failed to open JSONL file `{}`", path.display()))?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for (line_index, line_result) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line_result.with_context(|| {
            format!(
                "failed to read line {line_number} from `{}`",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str(&line).with_context(|| {
            format!(
                "failed to parse JSON on line {} of `{}`",
                line_number,
                path.display()
            )
        })?;
        records.push(record);
    }

    Ok(records)
}

#[derive(Deserialize, Serialize)]
struct TurnCompletedEventPayload {
    turn: TurnCompletedTurn,
}

#[derive(Deserialize, Serialize)]
struct TurnCompletedTurn {
    index: usize,
    assistant_response: String,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;
    use tempfile::tempdir;
    use uuid::Uuid;

    use crate::observability::event_log::{EventRecord, EventType};
    use crate::observability::trace::TraceRecord;

    use super::{
        PROJECT_DOC_INFLUENCE_OPERATION, PROJECT_DOC_READ_OPERATION, PROJECT_DOC_SEARCH_OPERATION,
        enrich, read_jsonl,
    };

    fn write_event_record(path: &Path, turn_index: usize, reply: &str) {
        let turn = json!({
            "index": turn_index,
            "started_at": "2026-06-07T00:00:00Z",
            "completed_at": "2026-06-07T00:00:01Z",
            "user_input": "test input",
            "context_assembly": {
                "selected": [],
                "omitted": [],
                "used_estimated_tokens": 0
            },
            "retrieved_memory_block": "",
            "assistant_response": reply,
            "recalled_turns": [],
            "model_id": "model",
            "model_latency_ms": 1,
            "input_tokens": 0,
            "cached_input_tokens": 0,
            "output_tokens": 0,
            "full_request_hash": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "message_count": 0
        });
        let event = EventRecord::new(
            "test-experiment",
            EventType::TurnCompleted,
            json!({
                "session_id": "test-session",
                "turn": turn,
                "full_request_hash": "00"
            }),
            None,
        );
        fs::write(path, serde_json::to_string(&event).unwrap() + "\n").unwrap();
    }

    fn write_trace_records(path: &Path, traces: &[TraceRecord]) {
        let mut contents = String::new();
        for trace in traces {
            contents.push_str(&serde_json::to_string(trace).unwrap());
            contents.push('\n');
        }
        fs::write(path, contents).unwrap();
    }

    fn search_trace(
        trace_id: Uuid,
        turn_index: usize,
        snippet: &str,
        refused: bool,
    ) -> TraceRecord {
        TraceRecord {
            trace_id,
            experiment_id: "test-experiment".to_string(),
            timestamp: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
            operation: PROJECT_DOC_SEARCH_OPERATION.to_string(),
            input_summary: "search".to_string(),
            output_summary: "1 hit(s)".to_string(),
            details: json!({
                "session_id": "test-session",
                "role_id": "role",
                "call_id": "call-search",
                "tool_name": "search_project_docs",
                "turn_index": turn_index,
                "arguments": { "query": "q", "max_results": 1 },
                "hits": [{
                    "path": "docs/ProjectFrame/ProjectVision.md",
                    "kind": "frame",
                    "maturity_tag": "accepted",
                    "last_reviewed": "2026-06-07",
                    "snippet": snippet,
                    "section_hint": "## Status",
                    "match_strength": "high"
                }],
                "hit_count": 1,
                "refused": refused,
            }),
            latency_domain: None,
            latency_stage: None,
            latency_ms: Some(1),
            latency_ns: Some(1_000_000),
            error: None,
        }
    }

    fn read_trace(trace_id: Uuid, turn_index: usize, content: &str, refused: bool) -> TraceRecord {
        TraceRecord {
            trace_id,
            experiment_id: "test-experiment".to_string(),
            timestamp: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
            operation: PROJECT_DOC_READ_OPERATION.to_string(),
            input_summary: "read".to_string(),
            output_summary: "is_full=true".to_string(),
            details: json!({
                "session_id": "test-session",
                "role_id": "role",
                "call_id": "call-read",
                "tool_name": "read_project_doc",
                "turn_index": turn_index,
                "arguments": { "path": "docs/ProjectFrame/ProjectVision.md", "focus": "status", "max_tokens": 200 },
                "read": {
                    "path": "docs/ProjectFrame/ProjectVision.md",
                    "kind": "frame",
                    "maturity_tag": "accepted",
                    "last_reviewed": "2026-06-07",
                    "content": content,
                    "is_full": true,
                    "omitted_sections": []
                },
                "is_full": true,
                "omitted_sections": [],
                "refused": refused,
            }),
            latency_domain: None,
            latency_stage: None,
            latency_ms: Some(1),
            latency_ns: Some(1_000_000),
            error: None,
        }
    }

    fn influence_source_ids(traces: &[TraceRecord]) -> Vec<Uuid> {
        traces
            .iter()
            .filter(|trace| trace.operation == PROJECT_DOC_INFLUENCE_OPERATION)
            .map(|trace| {
                trace.details["source_trace_id"]
                    .as_str()
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn enrichment_appends_influence_record_for_search() {
        let dir = tempdir().unwrap();
        let run_dir = dir.path();
        let events_path = run_dir.join("events.jsonl");
        let traces_path = run_dir.join("traces.jsonl");

        write_event_record(
            &events_path,
            0,
            "As the project's accepted framing says, that part is deferred.",
        );
        let source_trace_id = Uuid::new_v4();
        let source_trace = search_trace(
            source_trace_id,
            0,
            "The project's accepted framing says autonomy is deferred.",
            false,
        );
        write_trace_records(&traces_path, std::slice::from_ref(&source_trace));

        let appended = enrich(run_dir).unwrap();

        assert_eq!(appended, 1);
        let traces: Vec<TraceRecord> =
            read_jsonl(&traces_path).expect("traces should parse after enrichment");
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].trace_id, source_trace_id);
        assert_eq!(influence_source_ids(&traces), vec![source_trace_id]);
        let influence = traces
            .iter()
            .find(|trace| trace.operation == PROJECT_DOC_INFLUENCE_OPERATION)
            .unwrap();
        assert!(influence.details["influenced_reply"].as_bool().unwrap());
    }

    #[test]
    fn enrichment_appends_influence_record_for_read() {
        let dir = tempdir().unwrap();
        let run_dir = dir.path();
        let events_path = run_dir.join("events.jsonl");
        let traces_path = run_dir.join("traces.jsonl");

        write_event_record(
            &events_path,
            0,
            "The project's accepted framing says autonomy is deferred.",
        );
        let source_trace_id = Uuid::new_v4();
        let source_trace = read_trace(
            source_trace_id,
            0,
            "The project's accepted framing says autonomy is deferred.",
            false,
        );
        write_trace_records(&traces_path, std::slice::from_ref(&source_trace));

        let appended = enrich(run_dir).unwrap();

        assert_eq!(appended, 1);
        let traces: Vec<TraceRecord> =
            read_jsonl(&traces_path).expect("traces should parse after enrichment");
        let influence = traces
            .iter()
            .find(|trace| trace.operation == PROJECT_DOC_INFLUENCE_OPERATION)
            .unwrap();
        assert!(influence.details["influenced_reply"].as_bool().unwrap());
        assert_eq!(
            influence.details["source_trace_id"].as_str().unwrap(),
            source_trace_id.to_string()
        );
    }

    #[test]
    fn non_overlapping_reply_marks_not_influenced() {
        let dir = tempdir().unwrap();
        let run_dir = dir.path();
        let events_path = run_dir.join("events.jsonl");
        let traces_path = run_dir.join("traces.jsonl");

        write_event_record(&events_path, 0, "The capital of France is Paris.");
        let source_trace_id = Uuid::new_v4();
        let source_trace = read_trace(
            source_trace_id,
            0,
            "The project's accepted framing says autonomy is deferred.",
            false,
        );
        write_trace_records(&traces_path, std::slice::from_ref(&source_trace));

        let appended = enrich(run_dir).unwrap();

        assert_eq!(appended, 1);
        let traces: Vec<TraceRecord> =
            read_jsonl(&traces_path).expect("traces should parse after enrichment");
        let influence = traces
            .iter()
            .find(|trace| trace.operation == PROJECT_DOC_INFLUENCE_OPERATION)
            .unwrap();
        assert!(!influence.details["influenced_reply"].as_bool().unwrap());
    }

    #[test]
    fn refused_traces_are_skipped() {
        let dir = tempdir().unwrap();
        let run_dir = dir.path();
        let events_path = run_dir.join("events.jsonl");
        let traces_path = run_dir.join("traces.jsonl");

        write_event_record(
            &events_path,
            0,
            "As the project's accepted framing says, that part is deferred.",
        );
        let source_trace = search_trace(
            Uuid::new_v4(),
            0,
            "The project's accepted framing says autonomy is deferred.",
            true,
        );
        write_trace_records(&traces_path, &[source_trace]);

        let appended = enrich(run_dir).unwrap();

        assert_eq!(appended, 0);
        let traces: Vec<TraceRecord> =
            read_jsonl(&traces_path).expect("traces should parse after enrichment");
        assert_eq!(traces.len(), 1);
    }

    #[test]
    fn traces_without_same_turn_reply_are_skipped() {
        let dir = tempdir().unwrap();
        let run_dir = dir.path();
        let events_path = run_dir.join("events.jsonl");
        let traces_path = run_dir.join("traces.jsonl");

        fs::write(&events_path, "").unwrap();
        let source_trace = search_trace(
            Uuid::new_v4(),
            0,
            "The project's accepted framing says autonomy is deferred.",
            false,
        );
        write_trace_records(&traces_path, &[source_trace]);

        let appended = enrich(run_dir).unwrap();

        assert_eq!(appended, 0);
        let traces: Vec<TraceRecord> =
            read_jsonl(&traces_path).expect("traces should parse after enrichment");
        assert_eq!(traces.len(), 1);
    }

    #[test]
    fn enrich_is_idempotent() {
        let dir = tempdir().unwrap();
        let run_dir = dir.path();
        let events_path = run_dir.join("events.jsonl");
        let traces_path = run_dir.join("traces.jsonl");

        write_event_record(
            &events_path,
            0,
            "As the project's accepted framing says, that part is deferred.",
        );
        let source_trace = search_trace(
            Uuid::new_v4(),
            0,
            "The project's accepted framing says autonomy is deferred.",
            false,
        );
        write_trace_records(&traces_path, std::slice::from_ref(&source_trace));

        let first = enrich(run_dir).unwrap();
        let second = enrich(run_dir).unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 0);
        let traces: Vec<TraceRecord> =
            read_jsonl(&traces_path).expect("traces should parse after repeated enrichment");
        assert_eq!(traces.len(), 2);
        assert_eq!(influence_source_ids(&traces), vec![source_trace.trace_id]);
    }
}
