use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use crate::{
    DataSource, Dataset, GoldLabel, ReviewStatus, RosterSnapshot, UtteranceRosterAnnotation,
    build_report_for_dataset, default_dataset_path, default_roster_snapshot_path,
    parse_pair_results_jsonl, run_baseline, write_report_artifacts,
};

fn workspace_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn sample_dataset_path() -> PathBuf {
    workspace_path(default_dataset_path().to_str().expect("UTF-8 path"))
}

fn sample_roster_path() -> PathBuf {
    workspace_path(default_roster_snapshot_path().to_str().expect("UTF-8 path"))
}

fn sample_inputs() -> (Dataset, RosterSnapshot) {
    (
        Dataset::from_jsonl_path(sample_dataset_path()).expect("sample dataset parses"),
        RosterSnapshot::from_json_path(sample_roster_path()).expect("sample roster parses"),
    )
}

#[test]
fn sample_dataset_roundtrips_and_defaults_language() {
    let (dataset, _) = sample_inputs();
    assert_eq!(dataset.records[0].language, "en");
    let json = serde_json::to_string(&dataset.records[0]).expect("pair serializes");
    let reparsed = serde_json::from_str(&json).expect("pair reparses");
    assert_eq!(dataset.records[0], reparsed);
}

#[test]
fn sample_dataset_records_truthful_unreviewed_teacher_provenance() {
    let (dataset, _) = sample_inputs();
    assert!(dataset.records.iter().all(|record| {
        record.provenance.source == DataSource::Teacher
            && record.provenance.teacher_model_id.as_deref() == Some("gpt-5.6-terra")
            && record.provenance.review_status == ReviewStatus::Draft
    }));
}

#[test]
fn default_run_output_uses_repository_timestamp_convention() {
    let timestamp = time::OffsetDateTime::from_unix_timestamp(0).expect("Unix epoch is valid");
    assert_eq!(
        crate::report::run_output_dir_at(timestamp),
        PathBuf::from("runs/1970-01-01-000000-goal-relevance-sample")
    );
}

#[test]
fn unsupported_or_malformed_dataset_record_errors_loudly() {
    let source = fs::read_to_string(sample_dataset_path()).expect("read sample");
    let first_record = source.lines().next().expect("sample has a record");
    let mut off_version: Value = serde_json::from_str(first_record).expect("valid JSON");
    off_version["schema_version"] = Value::from(99);
    let off_version = serde_json::to_string(&off_version).expect("serialize JSON");
    assert!(
        Dataset::from_jsonl_str(&off_version, "off-version-test")
            .expect_err("unsupported version must fail")
            .contains("schema_version 99")
    );
    assert!(
        Dataset::from_jsonl_str("{not JSON}", "malformed-test")
            .expect_err("malformed record must fail")
            .contains("line 1")
    );
}

#[test]
fn none_of_roster_cannot_disagree_with_pair_label() {
    let source = fs::read_to_string(sample_dataset_path()).expect("read sample");
    let invalid = source
        .lines()
        .map(|line| {
            if line.contains("\"utterance_roster_annotation\":\"none_of_roster\"") {
                line.replacen(
                    "\"gold_label\":\"not_relevant\"",
                    "\"gold_label\":\"relevant\"",
                    1,
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let error = Dataset::from_jsonl_str(&invalid, "none-of-roster-test")
        .expect_err("a NoneOfRoster relevant pair must fail");
    assert!(error.contains("NoneOfRoster"));
}

#[test]
fn frozen_roster_roundtrips_and_guards_fixture_drift() {
    let (_, snapshot) = sample_inputs();
    let serialized = serde_json::to_string(&snapshot).expect("snapshot serializes");
    let reparsed: RosterSnapshot = serde_json::from_str(&serialized).expect("snapshot reparses");
    reparsed
        .validate()
        .expect("round-tripped snapshot validates");
    reparsed
        .assert_matches_current_realtime_seed()
        .expect("frozen realtime snapshot must match current fixture");
}

#[test]
fn runner_strengths_match_production_ranked_selection() {
    let (dataset, snapshot) = sample_inputs();
    let utterance = &dataset.records[0].utterance;
    let single = Dataset {
        records: snapshot
            .goals
            .iter()
            .map(|frozen_goal| {
                let mut record = dataset.records[0].clone();
                record.goal_ref = frozen_goal.goal_ref.clone();
                record
            })
            .collect(),
    };
    let results = run_baseline(&single, &snapshot).expect("runner scores fixture goals");
    let ranked =
        qsf_volition::select_goals_ranked(utterance, &snapshot.default_state, &snapshot.fixture);
    let ranked_strengths: BTreeMap<_, _> = ranked
        .selected
        .iter()
        .map(|selection| (selection.goal.id.as_str(), selection.match_strength))
        .collect();
    for (index, result) in results.iter().enumerate() {
        let goal = &snapshot.fixture.goals[index];
        assert_eq!(
            result.match_strength,
            ranked_strengths.get(goal.id.as_str()).copied().unwrap_or(0),
            "goal {}",
            goal.id
        );
    }
}

#[test]
fn ambiguous_pairs_are_excluded_and_none_of_roster_pairs_are_negative() {
    let (dataset, snapshot) = sample_inputs();
    let results = run_baseline(&dataset, &snapshot).expect("sample scores");
    let report = build_report_for_dataset(&results, &dataset).expect("report builds");
    assert_eq!(report.ambiguous_pair_count, 1);
    assert_eq!(report.none_of_roster_pair_count, 2);
    let operating = &report.production_operating_point;
    assert_eq!(
        operating.true_positive
            + operating.false_positive
            + operating.false_negative
            + operating.true_negative,
        (dataset.records.len() - report.ambiguous_pair_count as usize) as u32
    );
    assert_eq!(
        dataset
            .records
            .iter()
            .filter(|record| record.utterance_roster_annotation
                == UtteranceRosterAnnotation::NoneOfRoster)
            .filter(|record| record.gold_label == GoldLabel::NotRelevant)
            .count(),
        2
    );
}

#[test]
fn generated_artifacts_have_complete_trace_and_match_production_api() {
    let (dataset, snapshot) = sample_inputs();
    let results = run_baseline(&dataset, &snapshot).expect("sample scores");
    let report = build_report_for_dataset(&results, &dataset).expect("report builds");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let output = std::env::temp_dir().join(format!("qsf-semantic-eval-{unique}"));
    let artifacts = write_report_artifacts(&output, &results, &report).expect("artifacts write");
    assert!(artifacts.metrics_json.is_file());
    assert!(artifacts.metrics_summary.is_file());
    assert!(artifacts.error_analysis.is_file());

    let line = fs::read_to_string(&artifacts.pair_results_jsonl)
        .expect("result JSONL reads")
        .lines()
        .next()
        .expect("result JSONL has first record")
        .to_string();
    let parsed_value: Value = serde_json::from_str(&line).expect("result JSON parses");
    for field in [
        "dataset_version",
        "roster_snapshot_hash",
        "utterance",
        "goal_ref",
        "gold_label",
        "slice_tags",
        "matched_terms",
        "match_strength",
        "qualification_threshold_in_force",
        "qualifies_at_threshold_in_force",
        "scorer_source",
    ] {
        assert!(
            parsed_value.get(field).is_some(),
            "missing trace field {field}"
        );
    }
    assert!(parsed_value["qualification_threshold_in_force"].is_u64());

    let parsed_results =
        parse_pair_results_jsonl(&artifacts.pair_results_jsonl).expect("parse generated JSONL");
    let sampled = &parsed_results[0];
    let goal = snapshot
        .goal_for_ref(&sampled.goal_ref)
        .expect("goal reference resolves");
    let matched =
        qsf_volition::matched_keywords(goal, &qsf_volition::normalize_terms(&sampled.utterance));
    assert_eq!(
        sampled.match_strength,
        qsf_volition::match_strength(&matched)
    );
    assert_eq!(
        sampled.matched_terms,
        matched
            .into_iter()
            .map(|keyword| crate::MatchedTerm {
                term: keyword.term,
                weight_class: keyword.weight_class,
            })
            .collect::<Vec<_>>()
    );
}
