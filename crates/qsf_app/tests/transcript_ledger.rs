use std::path::PathBuf;

use qsf_app::transcript::{LedgerEntry, load_ledger, render_runs, runs_from_entries};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/realtime-diagnostics-sample.jsonl")
}

#[test]
fn a_real_ledger_excerpt_parses_with_no_skipped_lines() {
    let entries = load_ledger(&fixture_path()).expect("load the committed ledger excerpt");

    let skipped: Vec<&LedgerEntry> = entries
        .iter()
        .filter(|entry| matches!(entry, LedgerEntry::Skipped(_)))
        .collect();
    assert!(
        skipped.is_empty(),
        "every line of a real ledger must parse: {skipped:?}"
    );
    assert!(!entries.is_empty());
}

#[test]
fn a_real_ledger_excerpt_produces_turns_with_both_sides_and_a_threshold() {
    let entries = load_ledger(&fixture_path()).expect("load");
    let runs = runs_from_entries(entries, "fixture.jsonl", false);

    assert_eq!(runs.len(), 1, "the excerpt holds exactly one run");
    let run = &runs[0];
    assert!(
        run.header.source.complete,
        "a real excerpt must read completely: {:?}",
        run.header.source
    );
    assert!(!run.turns.is_empty());

    let first = &run.turns[0];
    assert!(!first.user.is_empty(), "a trusted turn carries user text");
    assert!(
        first.assistant.is_some(),
        "a trusted turn carries a response"
    );
    assert!(first.undecodable.is_empty());

    let volition = first
        .volition
        .as_ref()
        .expect("a trusted turn carries an injection trace");
    assert!(
        volition.threshold > 0,
        "the qualification threshold is recorded"
    );
}

#[test]
fn compact_rendering_of_a_real_ledger_is_valid_jsonl() {
    let entries = load_ledger(&fixture_path()).expect("load");
    let runs = runs_from_entries(entries, "fixture.jsonl", false);
    let rendered = render_runs(runs, false).expect("render");

    for line in rendered.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("each emitted line is valid JSON");
    }
}

#[test]
fn curated_rendering_of_a_real_ledger_contains_no_floating_point() {
    let entries = load_ledger(&fixture_path()).expect("load");
    let runs = runs_from_entries(entries, "fixture.jsonl", false);
    let rendered = render_runs(runs, false).expect("render");

    for line in rendered.lines() {
        let value = serde_json::from_str::<serde_json::Value>(line).expect("valid JSON");
        assert!(
            !contains_float(&value),
            "curated output must contain no floating-point numbers: {value}"
        );
    }
}

fn contains_float(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(number) => number.is_f64(),
        serde_json::Value::Array(items) => items.iter().any(contains_float),
        serde_json::Value::Object(fields) => fields.values().any(contains_float),
        _ => false,
    }
}

/// The artifact must carry its own provenance: a reader who has only the file, and never saw the
/// invocation's stderr, has to be able to tell that a line was skipped.
#[test]
fn a_partially_read_ledger_says_so_in_the_written_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("partial.jsonl");
    let fixture = std::fs::read_to_string(fixture_path()).expect("read fixture");
    std::fs::write(
        &path,
        format!(
            "{fixture}{}\n",
            r#"{"kind":"from_a_future_build","qsf_session_id":"default","exchange_index":0}"#
        ),
    )
    .expect("write");

    let path_label = path.display().to_string();
    let runs = runs_from_entries(load_ledger(&path).expect("load"), &path_label, false);
    let rendered = render_runs(runs, false).expect("render");

    let session: serde_json::Value =
        serde_json::from_str(rendered.lines().next().expect("a session line")).expect("parse");
    assert_eq!(session["source"]["complete"], serde_json::json!(false));
    assert_eq!(
        session["source"]["skipped_line_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        session["source"]["skipped_lines"][0]["kind"],
        serde_json::json!("from_a_future_build")
    );

    let turn: serde_json::Value =
        serde_json::from_str(rendered.lines().nth(1).expect("a turn line")).expect("parse");
    assert_eq!(
        turn["undecodable"],
        serde_json::json!(["from_a_future_build"]),
        "the turn the undecodable line belonged to must be marked"
    );
}
