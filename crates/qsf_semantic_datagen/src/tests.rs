use std::{path::PathBuf, process::Command};

use qsf_semantic_eval::{GoldLabel, ReviewStatus, RosterSnapshot};
use serde_json::Value;

use crate::{
    CompletionRequest, FixtureResponse, GenerationOutput, INTERCHANGE_VERSION, LabelInterchange,
    ModelTransport, PerGoalLabel, ReplayTransport, ReviewDecision, ReviewField, ReviewValue,
    ReviewedPoolRequest, TransportKind, build_labeling_input, default_transport_kind,
    fold_reviewed_pool, parse_generation_output, parse_label_interchange, reconcile, write_jsonl,
};

fn roster() -> RosterSnapshot {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json");
    RosterSnapshot::from_json_path(path).expect("frozen roster parses")
}

fn generated() -> GenerationOutput {
    GenerationOutput {
        interchange_version: INTERCHANGE_VERSION,
        utterance_id: "utterance-1".to_string(),
        utterance: "Could automation change work?".to_string(),
        language: "en".to_string(),
        conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
        intended_slice_tags: vec![qsf_semantic_eval::SliceTag::HardNegative],
        session_id: "session-1".to_string(),
        semantic_cluster_id: "cluster-1".to_string(),
        generation_run_id: "generation-1".to_string(),
        generator_model_id: "gpt-5.4-nano".to_string(),
        prompt_version: "goalrel-gen-v1".to_string(),
        saw_activation_keywords: false,
    }
}

fn label(labeler_id: &str, label: GoldLabel) -> LabelInterchange {
    let roster = roster();
    LabelInterchange {
        interchange_version: INTERCHANGE_VERSION,
        labeler_id: labeler_id.to_string(),
        labeling_run_id: format!("{labeler_id}-run"),
        guideline_version: "goalrel-label-v1".to_string(),
        utterance_id: "utterance-1".to_string(),
        per_goal: vec![PerGoalLabel {
            goal_ref: roster.goals[0].goal_ref.clone(),
            label,
        }],
        none_of_roster: false,
    }
}

#[test]
fn blind_labeling_input_cannot_leak_pool_metadata() {
    let input = build_labeling_input(&[generated()], &roster()).expect("build blind input");
    let json = write_jsonl(&input).expect("serialize blind input");
    let value: Value = serde_json::from_str(json.lines().next().expect("record")).expect("json");
    for forbidden in [
        "gold_label",
        "slice_tags",
        "conditioning_goal_ref",
        "generation_run_id",
        "generator_model_id",
        "prompt_version",
        "saw_activation_keywords",
    ] {
        assert!(
            value.get(forbidden).is_none(),
            "blinding leaked {forbidden}"
        );
    }
    let reparsed = crate::parse_labeling_input(&json).expect("blind input round trips");
    assert_eq!(reparsed, input);
}

#[test]
fn label_validator_accepts_both_labelers_and_rejects_relevant_none_of_roster() {
    let roster = roster();
    for labeler in [
        label("gpt-5.4-mini", GoldLabel::Relevant),
        label("claude-fable", GoldLabel::Relevant),
    ] {
        let json = write_jsonl(&[labeler]).expect("serialize label");
        parse_label_interchange(&json, &roster).expect("well formed label validates");
    }
    let mut invalid = label("gpt-5.4-mini", GoldLabel::Relevant);
    invalid.none_of_roster = true;
    let error = crate::validate_label_interchange(&[invalid], &roster)
        .expect_err("conflicting none of roster must fail");
    assert!(error.contains("none_of_roster"));
}

#[test]
fn reconciliation_is_deterministic_and_marks_disagreement() {
    let mini = label("gpt-5.4-mini", GoldLabel::Relevant);
    let fable = label("claude-fable", GoldLabel::NotRelevant);
    let first =
        reconcile(std::slice::from_ref(&mini), std::slice::from_ref(&fable)).expect("reconcile");
    assert_eq!(
        first,
        reconcile(&[mini], &[fable]).expect("reconcile again")
    );
    assert!(!first[0].agree);
}

#[test]
fn reviewed_pool_fold_is_last_decision_wins_after_explicit_coverage() {
    let roster = roster();
    let mini = label("gpt-5.4-mini", GoldLabel::Relevant);
    let fable = label("claude-fable", GoldLabel::Relevant);
    let goal_ref = roster.goals[0].goal_ref.clone();
    let decisions = vec![
        ReviewDecision {
            decided_at: "2026-07-20T00:00:00Z".to_string(),
            utterance_id: "utterance-1".to_string(),
            goal_ref: Some(goal_ref.clone()),
            field: ReviewField::GoldLabel,
            value: ReviewValue::GoldLabel(GoldLabel::NotRelevant),
        },
        ReviewDecision {
            decided_at: "2026-07-20T00:01:00Z".to_string(),
            utterance_id: "utterance-1".to_string(),
            goal_ref: Some(goal_ref.clone()),
            field: ReviewField::GoldLabel,
            value: ReviewValue::GoldLabel(GoldLabel::Ambiguous),
        },
        ReviewDecision {
            decided_at: "2026-07-20T00:02:00Z".to_string(),
            utterance_id: "utterance-1".to_string(),
            goal_ref: None,
            field: ReviewField::NoneOfRoster,
            value: ReviewValue::NoneOfRoster(false),
        },
    ];
    let request = ReviewedPoolRequest {
        generated: &[generated()],
        roster: &roster,
        mini: &[mini],
        fable: &[fable],
        decisions: &decisions,
        dataset_version: "pool-v2",
        generation_output_sha256: "sha256:g",
        label_mini_sha256: "sha256:m",
        label_fable_sha256: "sha256:f",
        review_decisions_sha256: "sha256:r",
    };
    let pool = fold_reviewed_pool(request).expect("fold decisions");
    assert_eq!(pool[0].gold_label, GoldLabel::Ambiguous);
    assert_eq!(
        pool[0].provenance.review.review_status,
        ReviewStatus::Reviewed
    );
}

#[test]
fn uncovered_disagreement_remains_a_draft_with_the_mini_label() {
    let roster = roster();
    let generated_records = [generated()];
    let mini = [label("gpt-5.4-mini", GoldLabel::Relevant)];
    let fable = [label("claude-fable", GoldLabel::NotRelevant)];
    let pool = fold_reviewed_pool(ReviewedPoolRequest {
        generated: &generated_records,
        roster: &roster,
        mini: &mini,
        fable: &fable,
        decisions: &[],
        dataset_version: "pool-v2",
        generation_output_sha256: "sha256:g",
        label_mini_sha256: "sha256:m",
        label_fable_sha256: "sha256:f",
        review_decisions_sha256: "sha256:empty",
    })
    .expect("draft pool remains schema-valid");

    assert_eq!(pool[0].gold_label, GoldLabel::Relevant);
    assert_eq!(pool[0].provenance.review.review_status, ReviewStatus::Draft);
}

#[test]
fn pair_decision_without_utterance_annotation_decision_remains_draft() {
    let roster = roster();
    let generated_records = [generated()];
    let mini = [label("gpt-5.4-mini", GoldLabel::Relevant)];
    let fable = [label("claude-fable", GoldLabel::Relevant)];
    let decision = [ReviewDecision {
        decided_at: "2026-07-20T00:00:00Z".to_string(),
        utterance_id: "utterance-1".to_string(),
        goal_ref: Some(roster.goals[0].goal_ref.clone()),
        field: ReviewField::GoldLabel,
        value: ReviewValue::GoldLabel(GoldLabel::Relevant),
    }];
    let pool = fold_reviewed_pool(ReviewedPoolRequest {
        generated: &generated_records,
        roster: &roster,
        mini: &mini,
        fable: &fable,
        decisions: &decision,
        dataset_version: "pool-v2",
        generation_output_sha256: "sha256:g",
        label_mini_sha256: "sha256:m",
        label_fable_sha256: "sha256:f",
        review_decisions_sha256: "sha256:pair-only",
    })
    .expect("partially reviewed pool remains schema-valid");

    assert_eq!(pool[0].provenance.review.review_status, ReviewStatus::Draft);
}

#[test]
fn default_transport_replays_fixtures_that_pass_real_parsers_and_validators() {
    assert_eq!(default_transport_kind(), TransportKind::Replay);
    let request = CompletionRequest {
        model_id: "fixture-model".to_string(),
        prompt: "this prompt must not be echoed".to_string(),
        goal_ref: "fixture-goal".to_string(),
        run_id: "fixture-run".to_string(),
    };

    let generation = ReplayTransport::new(FixtureResponse::Generation)
        .complete(&request)
        .expect("generation fixture replays");
    assert_ne!(generation, request.prompt);
    let generated = parse_generation_output(&generation).expect("generation fixture validates");
    assert_eq!(generated.len(), 1);

    for (kind, expected_labeler) in [
        (FixtureResponse::MiniLabel, "gpt-5.4-mini"),
        (FixtureResponse::FableLabel, "claude-fable"),
    ] {
        let response = ReplayTransport::new(kind)
            .complete(&request)
            .expect("label fixture replays");
        assert_ne!(response, request.prompt);
        let labels = parse_label_interchange(&response, &roster())
            .expect("label fixture parses and validates against roster");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].labeler_id, expected_labeler);
        assert_eq!(labels[0].per_goal.len(), roster().goals.len());
    }
}

#[test]
fn semantic_eval_dependency_graph_stays_lean() {
    let output = Command::new("cargo")
        .args(["tree", "-p", "qsf_semantic_eval"])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).expect("cargo tree UTF-8");
    for forbidden in ["openai_provider_kit", "reqwest", "tokio"] {
        assert!(
            !tree.contains(forbidden),
            "qsf_semantic_eval dependency tree must not contain {forbidden}: {tree}"
        );
    }
}
