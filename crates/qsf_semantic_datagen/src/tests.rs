use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use qsf_semantic_eval::{GoldLabel, ReviewStatus, RosterSnapshot};
use serde_json::Value;

use crate::{
    CompletionRequest, FixtureResponse, GOAL_RELEVANCE_GUIDELINE_VERSION, GenerationClusterAnchor,
    GenerationMode, GenerationOutput, GenerationResponseContext, GoalDescription,
    INTERCHANGE_VERSION, LabelInterchange, LabelingResponseContext, MINI_LABELER_ID,
    ModelTransport, PerGoalLabel, PromptRequest, ReplayTransport, ReviewDecision, ReviewField,
    ReviewValue, ReviewedPoolRequest, TransportKind, build_blind_qa_review_view_model,
    build_goal_relevance_label_prompt, build_labeling_input, build_prompt, build_review_view_model,
    cluster_scenario_directive, default_transport_kind, fold_reviewed_pool, hard_negative_count,
    hard_negative_within_distribution, mini_fable_agreement_rate, parse_generation_anchor_sidecar,
    parse_generation_output, parse_generation_response, parse_goal_relevance_label_response,
    parse_label_interchange, priority_review_queue, punctuation_casing_loss, reconcile,
    render_blind_qa_review_view, render_generation_report, run_cli, run_generation,
    run_generation_anchors, run_generation_with_approved_anchors, run_mini_labeling,
    split_feasibility_preflight, synthetic_asr_corrupt, tool_local_price_table,
    validate_generation_output, write_generation_anchor_sidecar, write_jsonl,
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
        intended_slice_tags: Vec::new(),
        session_id: "session-1".to_string(),
        semantic_cluster_id: "cluster-1".to_string(),
        generation_run_id: "generation-1".to_string(),
        generator_model_id: "gpt-5.4-nano".to_string(),
        prompt_version: crate::GENERATION_PROMPT_VERSION.to_string(),
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

fn full_label(labeler_id: &str, labels: &[GoldLabel]) -> LabelInterchange {
    let roster = roster();
    assert_eq!(labels.len(), roster.goals.len());
    LabelInterchange {
        interchange_version: INTERCHANGE_VERSION,
        labeler_id: labeler_id.to_string(),
        labeling_run_id: format!("{labeler_id}-run"),
        guideline_version: GOAL_RELEVANCE_GUIDELINE_VERSION.to_string(),
        utterance_id: "utterance-1".to_string(),
        per_goal: roster
            .goals
            .iter()
            .zip(labels)
            .map(|(goal, label)| PerGoalLabel {
                goal_ref: goal.goal_ref.clone(),
                label: label.clone(),
            })
            .collect(),
        none_of_roster: false,
    }
}

fn approved_anchors() -> Vec<GenerationClusterAnchor> {
    [
        "validation-cluster-1",
        "validation-cluster-2",
        "validation-cluster-3",
        "validation-cluster-4",
        "validation-hard-cluster",
        "test-cluster-1",
        "test-cluster-2",
        "test-cluster-3",
        "test-cluster-4",
        "test-hard-cluster",
    ]
    .into_iter()
    .map(|cluster_id| GenerationClusterAnchor {
        cluster_id: cluster_id.to_string(),
        anchor: format!("The user chooses a permitted topic in {cluster_id}."),
    })
    .collect()
}

struct RecordingTransport {
    requests: std::cell::RefCell<Vec<CompletionRequest>>,
    usage: Option<crate::TokenUsage>,
}

impl RecordingTransport {
    fn new(usage: Option<crate::TokenUsage>) -> Self {
        Self {
            requests: std::cell::RefCell::new(Vec::new()),
            usage,
        }
    }

    fn prompts(&self) -> Vec<String> {
        self.requests
            .borrow()
            .iter()
            .map(|request| request.prompt.clone())
            .collect()
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.borrow().clone()
    }
}

impl ModelTransport for RecordingTransport {
    fn complete(&self, request: &CompletionRequest) -> Result<crate::Completion, String> {
        self.requests.borrow_mut().push(request.clone());
        Ok(crate::Completion {
            content: ReplayTransport::default_response(FixtureResponse::Generation)
                .expect("generation fixture readable"),
            usage: self.usage,
        })
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
fn mini_label_prompt_is_blind_to_generator_intent_slice_tags_and_provenance() {
    let mut record = generated();
    record.conditioning_goal_ref = Some("intended-goal-secret".to_string());
    record.intended_slice_tags = vec![qsf_semantic_eval::SliceTag::ExplicitNegation];
    record.generation_run_id = "generation-provenance-secret".to_string();
    record.generator_model_id = "generator-model-secret".to_string();
    let input = build_labeling_input(&[record], &roster()).expect("build blind input");
    let prompt = build_goal_relevance_label_prompt(&input[0]).expect("build blind mini prompt");
    for forbidden in [
        "intended-goal-secret",
        "explicit_negation",
        "generation-provenance-secret",
        "generator-model-secret",
    ] {
        assert!(!prompt.contains(forbidden), "prompt leaked {forbidden}");
    }
}

#[test]
fn replay_mini_labeling_builds_and_validates_a_complete_interchange_file() {
    assert_eq!(default_transport_kind(), TransportKind::Replay);
    let input = build_labeling_input(&[generated()], &roster()).expect("build input");
    let run = run_mini_labeling(
        &ReplayTransport::new(FixtureResponse::MiniLabel),
        &input,
        "fixture-mini-label-run",
    )
    .expect("replay mini labeling completes without network");
    assert!(run.usage.is_none());
    let serialized = write_jsonl(&run.labels).expect("write label-mini artifact");
    let labels = parse_label_interchange(&serialized, &roster()).expect("shared validation");
    assert_eq!(labels[0].labeler_id, MINI_LABELER_ID);
    assert_eq!(labels[0].labeling_run_id, "fixture-mini-label-run");
    assert_eq!(
        labels[0].guideline_version,
        GOAL_RELEVANCE_GUIDELINE_VERSION
    );
    assert_eq!(labels[0].utterance_id, "utterance-1");
    assert_eq!(labels[0].per_goal.len(), roster().goals.len());
}

#[test]
fn replay_generation_run_produces_the_valid_generation_pool_contract() {
    let run = run_generation(
        &ReplayTransport::new(FixtureResponse::Generation),
        &roster(),
        "fixture-generation-contract",
    )
    .expect("replay generation completes without network");
    assert!(run.usage_by_model.is_none());
    assert_eq!(run.records.len(), 248);
    assert_eq!(run.cluster_anchors.len(), 10);
    assert_eq!(crate::GENERATION_PROMPT_VERSION, "goalrel-gen-v6");
    assert!(
        run.records
            .iter()
            .all(|record| record.prompt_version == "goalrel-gen-v6")
    );
    assert!(run.cluster_anchors.iter().all(|cluster| cluster.anchor
        == "A user imagines named people following another person's conversational lead."));
    for cluster in &run.cluster_anchors {
        assert!(
            cluster_scenario_directive(&cluster.cluster_id).is_some(),
            "cluster {} resolves to a partition-keyed directive",
            cluster.cluster_id
        );
    }
    validate_generation_output(&run.records).expect("generated pool validates");
    let jsonl = write_jsonl(&run.records).expect("serialize generated pool");
    let reparsed = parse_generation_output(&jsonl).expect("parse generated pool artifact");
    assert_eq!(reparsed, run.records);
}

#[test]
fn generation_selects_models_by_mode_and_aggregates_usage_by_model() {
    assert_eq!(GenerationMode::Natural.generator_model_id(), "gpt-5.4-nano");
    for mode in [
        GenerationMode::ParaphraseCluster {
            cluster_id: "validation-cluster-1".to_string(),
        },
        GenerationMode::SubjectConfusion,
        GenerationMode::HardParaphrase {
            cluster_id: "validation-hard-cluster".to_string(),
        },
        GenerationMode::VagueNoneOfRoster,
    ] {
        assert_eq!(mode.generator_model_id(), "gpt-5.4-mini");
    }
    let transport = RecordingTransport::new(Some(crate::TokenUsage {
        input_tokens: 2,
        cached_input_tokens: 1,
        output_tokens: 3,
    }));
    let run =
        run_generation(&transport, &roster(), "model-selection-run").expect("generation completes");
    let usage = run.usage_by_model.expect("usage is aggregated");
    assert_eq!(
        usage.get("gpt-5.4-mini"),
        Some(&crate::TokenUsage {
            input_tokens: 28,
            cached_input_tokens: 14,
            output_tokens: 42,
        })
    );
    assert_eq!(
        usage.get("gpt-5.4-nano"),
        Some(&crate::TokenUsage {
            input_tokens: 32,
            cached_input_tokens: 16,
            output_tokens: 48,
        })
    );
    let requests = transport.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.model_id == "gpt-5.4-mini")
            .count(),
        14
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.model_id == "gpt-5.4-nano")
            .count(),
        16
    );
    assert_eq!(
        run.records
            .iter()
            .filter(|record| record.generator_model_id == "gpt-5.4-mini")
            .count(),
        120
    );
    assert_eq!(
        run.records
            .iter()
            .filter(|record| record.generator_model_id == "gpt-5.4-nano")
            .count(),
        128
    );
    let vague_per_partition = run
        .records
        .iter()
        .filter(|record| record.conditioning_goal_ref.is_none())
        .fold(std::collections::BTreeMap::new(), |mut counts, record| {
            *counts.entry(&record.session_id).or_insert(0usize) += 1;
            counts
        });
    assert_eq!(
        vague_per_partition.values().copied().collect::<Vec<_>>(),
        [12, 12]
    );

    let anchor_transport = RecordingTransport::new(None);
    let anchors = run_generation_anchors(&anchor_transport, &roster(), "anchor-model-run")
        .expect("anchor checkpoint completes");
    assert_eq!(anchors.cluster_anchors.len(), 10);
    assert!(
        anchor_transport
            .requests()
            .iter()
            .all(|request| request.model_id == "gpt-5.4-mini")
    );
}

#[test]
fn cluster_anchor_sidecar_round_trips_and_rejects_unknown_fields() {
    let anchors = vec![GenerationClusterAnchor {
        cluster_id: "validation-cluster-1".to_string(),
        anchor: "The user names a limit before a meeting.".to_string(),
    }];
    let jsonl = write_generation_anchor_sidecar(&anchors).expect("anchor sidecar serializes");
    assert_eq!(parse_generation_anchor_sidecar(&jsonl), Ok(anchors));
    let error = parse_generation_anchor_sidecar(
        r#"{"cluster_id":"validation-cluster-1","anchor":"A proposition","unexpected":true}"#,
    )
    .expect_err("anchor sidecar rejects unknown fields");
    assert!(error.contains("unknown field"), "{error}");
}

#[test]
fn replay_generate_cli_writes_one_anchor_sidecar_entry_per_cluster_batch() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("qsf-anchor-sidecar-{unique}"));
    fs::create_dir_all(&output_dir).expect("test output directory");
    let output_path = output_dir.join("generation-output.jsonl");
    let roster_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json");
    run_cli(
        [
            "generate".to_string(),
            roster_path.display().to_string(),
            output_path.display().to_string(),
        ]
        .into_iter(),
    )
    .expect("replay generate CLI succeeds");
    let anchors_path = crate::transport::generation_anchor_sidecar_path(&output_path);
    assert_eq!(
        anchors_path,
        output_dir.join("generation-anchors.jsonl"),
        "sidecar stays next to the output with its fixed name"
    );
    let anchors = parse_generation_anchor_sidecar(
        &fs::read_to_string(&anchors_path).expect("anchor sidecar was written"),
    )
    .expect("anchor sidecar parses");
    assert_eq!(anchors.len(), 10, "one entry per cluster batch");
    fs::remove_dir_all(&output_dir).expect("remove test output directory");
}

#[test]
fn replay_anchors_only_cli_writes_ten_validated_checkpoint_records() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("qsf-anchors-only-{unique}"));
    fs::create_dir_all(&output_dir).expect("test output directory");
    let anchors_path = output_dir.join("approved-anchors.jsonl");
    let roster_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json");
    run_cli(
        [
            "generate".to_string(),
            "--anchors-only".to_string(),
            roster_path.display().to_string(),
            anchors_path.display().to_string(),
        ]
        .into_iter(),
    )
    .expect("replay anchors-only CLI succeeds");
    let anchors = parse_generation_anchor_sidecar(
        &fs::read_to_string(&anchors_path).expect("checkpoint was written"),
    )
    .expect("checkpoint sidecar validates");
    assert_eq!(anchors.len(), 10);
    crate::validate_approved_generation_anchors(&anchors).expect("all expected clusters present");
    fs::remove_dir_all(&output_dir).expect("remove test output directory");
}

#[test]
fn approved_anchor_validation_rejects_a_missing_cluster_id() {
    let missing_id = "validation-cluster-3";
    let anchors = approved_anchors()
        .into_iter()
        .filter(|anchor| anchor.cluster_id != missing_id)
        .collect::<Vec<_>>();
    let error = crate::validate_approved_generation_anchors(&anchors)
        .expect_err("a missing scheduled cluster must be rejected");
    assert!(error.contains(missing_id), "{error}");
}

#[test]
fn approved_anchor_validation_rejects_an_extra_cluster_id() {
    let extra_id = "validation-cluster-extra";
    let mut anchors = approved_anchors();
    anchors.push(GenerationClusterAnchor {
        cluster_id: extra_id.to_string(),
        anchor: "An extra proposition.".to_string(),
    });
    let error = crate::validate_approved_generation_anchors(&anchors)
        .expect_err("an unscheduled cluster must be rejected");
    assert!(error.contains(extra_id), "{error}");
}

#[test]
fn approved_anchor_validation_rejects_a_duplicate_cluster_id() {
    let duplicate_id = "test-hard-cluster";
    let mut anchors = approved_anchors();
    anchors.push(
        anchors
            .iter()
            .find(|anchor| anchor.cluster_id == duplicate_id)
            .expect("duplicate source anchor exists")
            .clone(),
    );
    let error = crate::validate_approved_generation_anchors(&anchors)
        .expect_err("a duplicate scheduled cluster must be rejected");
    assert!(error.contains(duplicate_id), "{error}");
}

#[test]
fn approved_anchors_are_embedded_and_cli_passes_the_sidecar_through_unchanged() {
    let anchors = approved_anchors();
    let transport = RecordingTransport::new(None);
    let run = run_generation_with_approved_anchors(
        &transport,
        &roster(),
        "approved-anchor-run",
        Some(&anchors),
    )
    .expect("generation with approved anchors succeeds");
    assert_eq!(run.records.len(), 248);
    let prompts = transport.prompts();
    let cluster_prompts = prompts
        .iter()
        .filter(|prompt| prompt.contains("operator-approved anchor proposition"))
        .collect::<Vec<_>>();
    assert_eq!(cluster_prompts.len(), 10);
    for prompt in &cluster_prompts {
        assert!(prompt.contains("Required scenario directive for this cluster:"));
        for forbidden in [
            "The anchor must use this required combination.",
            "Previously fixed anchors in this generation run:",
            "Your anchor must differ in stance, actors, action, and consequence",
        ] {
            assert!(
                !prompt.contains(forbidden),
                "approved-anchor prompt contains authoring guidance {forbidden}: {prompt}"
            );
        }
    }
    for anchor in &anchors {
        assert!(
            cluster_prompts
                .iter()
                .any(|prompt| prompt.contains(&anchor.anchor)),
            "approved anchor {} was not embedded",
            anchor.cluster_id
        );
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("qsf-approved-anchors-{unique}"));
    fs::create_dir_all(&output_dir).expect("test output directory");
    let approved_path = output_dir.join("approved.jsonl");
    let approved_content = write_generation_anchor_sidecar(&anchors).expect("serialize anchors");
    fs::write(&approved_path, &approved_content).expect("write approved checkpoint");
    let output_path = output_dir.join("generation-output.jsonl");
    let roster_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json");
    run_cli(
        [
            "generate".to_string(),
            "--anchors".to_string(),
            approved_path.display().to_string(),
            roster_path.display().to_string(),
            output_path.display().to_string(),
        ]
        .into_iter(),
    )
    .expect("approved-anchor replay CLI succeeds");
    let sidecar_path = crate::transport::generation_anchor_sidecar_path(&output_path);
    assert_eq!(
        fs::read_to_string(&sidecar_path).expect("pass-through sidecar written"),
        approved_content
    );
    assert_eq!(
        parse_generation_output(&fs::read_to_string(&output_path).expect("output written"))
            .expect("output validates")
            .len(),
        248
    );
    fs::remove_dir_all(&output_dir).expect("remove test output directory");
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
fn priority_review_queue_places_planted_disagreements_first_and_reports_agreement_rate() {
    let mini = full_label(
        "gpt-5.4-mini",
        &[
            GoldLabel::Relevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
        ],
    );
    let fable = full_label(
        "claude-fable",
        &[
            GoldLabel::Ambiguous,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
            GoldLabel::NotRelevant,
        ],
    );
    let reconciliation = reconcile(&[mini], &[fable]).expect("reconcile planted disagreement");
    let queue = priority_review_queue(&reconciliation);
    assert!(!queue[0].agree);
    assert_eq!(queue[0].goal_ref, roster().goals[0].goal_ref);
    let agreement = mini_fable_agreement_rate(&reconciliation);
    assert_eq!(agreement.agreed_pairs, 6);
    assert_eq!(agreement.total_pairs, 7);
    assert_eq!(agreement.rate, 6.0 / 7.0);
}

#[test]
fn review_view_shows_all_roster_labels_and_blind_qa_omits_them() {
    let input = build_labeling_input(&[generated()], &roster()).expect("build input");
    let labels = vec![GoldLabel::NotRelevant; roster().goals.len()];
    let mini = full_label("gpt-5.4-mini", &labels);
    let fable = full_label("claude-fable", &labels);
    let review = build_review_view_model("utterance-1", &input, &[mini], &[fable], &[])
        .expect("build review view");
    assert_eq!(review.per_goal.len(), 7);
    let blind =
        build_blind_qa_review_view_model("utterance-1", &input).expect("build blind QA view");
    assert_eq!(blind.per_goal.len(), 7);
    assert!(blind.asks_for_none_of_roster);
    let rendered = render_blind_qa_review_view(&blind);
    assert!(
        !rendered.contains("label"),
        "blind view leaked a label: {rendered}"
    );
}

#[test]
fn blind_qa_uses_a_distinct_artifact_and_cannot_replace_review_decisions() {
    let review_path = PathBuf::from("lineage/review-decisions.jsonl");
    assert_eq!(
        crate::transport::review_output_path(&review_path, false),
        review_path
    );
    assert_eq!(
        crate::transport::review_output_path(&review_path, true),
        PathBuf::from("lineage/blind-qa-decisions.jsonl")
    );
}

#[test]
fn conflicting_none_of_roster_answer_is_reprompted_without_losing_goal_answers() {
    let decisions = vec![ReviewDecision {
        decided_at: "now".to_string(),
        utterance_id: "utterance-1".to_string(),
        goal_ref: Some(roster().goals[0].goal_ref.clone()),
        field: ReviewField::GoldLabel,
        value: ReviewValue::GoldLabel(GoldLabel::Relevant),
    }];
    assert_eq!(
        crate::transport::review_none_of_roster_action(&decisions, true),
        crate::transport::NoneOfRosterAction::Reprompt
    );
    assert_eq!(
        crate::transport::review_none_of_roster_action(&decisions, false),
        crate::transport::NoneOfRosterAction::Accept(false)
    );
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
    let decision_file = write_jsonl(&decisions).expect("record decisions JSONL");
    let recorded_decisions =
        crate::parse_review_decisions(&decision_file).expect("parse decisions");
    let request = ReviewedPoolRequest {
        generated: &[generated()],
        roster: &roster,
        mini: &[mini],
        fable: &[fable],
        decisions: &recorded_decisions,
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
        utterance_id: None,
    };
    let input = build_labeling_input(&[generated()], &roster()).expect("build labeling input");

    let generation = ReplayTransport::new(FixtureResponse::Generation)
        .complete(&request)
        .expect("generation fixture replays");
    assert!(generation.usage.is_none());
    assert_ne!(generation.content, request.prompt);
    let generated = parse_generation_response(
        &generation.content,
        &GenerationResponseContext {
            utterance_id_prefix: "fixture".to_string(),
            language: "en".to_string(),
            conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
            mode: GenerationMode::Hypothetical,
            session_id: "fixture-session".to_string(),
            semantic_cluster_id: "fixture-cluster".to_string(),
            generation_run_id: "fixture-run".to_string(),
            generator_model_id: "gpt-5.4-nano".to_string(),
            synthetic_asr_seed: 7,
        },
    )
    .expect("generation fixture parses");
    let expected_utterances = [
        "Imagine Maya focuses on the project plans after a coworker grows quiet.",
        "What if OpenAI gets a gentle nudge from Jordan about the meeting agenda?",
        "Suppose Priya shifts the topic toward the release plan after Sam hesitates.",
        "If I ever hear Maya pause, I would follow her lead with the schedule.",
        "Imagine Nvidia receives a request for the public timeline from Jordan.",
        "What if Lena describes the deliverables while Omar waits for an answer?",
        "Suppose Alex stays with the design notes after Priya changes the subject.",
        "If I were Casey, I would ask Lee about the proposal.",
        "Imagine Maya schedules a library visit.",
        "What if Jordan chooses a blue notebook?",
        "Suppose Priya waters the plants on Saturday.",
        "If I were Casey, I would bake bread this evening.",
    ];
    let actual: Vec<_> = generated
        .iter()
        .map(|record| (record.utterance_id.clone(), record.utterance.as_str()))
        .collect();
    let expected: Vec<_> = expected_utterances
        .iter()
        .enumerate()
        .map(|(index, utterance)| (format!("fixture-{:02}", index + 1), *utterance))
        .collect();
    assert_eq!(actual, expected);
    assert!(
        generated
            .iter()
            .all(|record| !record.saw_activation_keywords)
    );

    for (kind, expected_labeler) in [
        (FixtureResponse::MiniLabel, "gpt-5.4-mini"),
        (FixtureResponse::FableLabel, "claude-fable"),
    ] {
        let response = ReplayTransport::new(kind)
            .complete(&request)
            .expect("label fixture replays");
        assert_ne!(response.content, request.prompt);
        let label = parse_goal_relevance_label_response(
            &response.content,
            LabelingResponseContext {
                input: &input[0],
                labeler_id: expected_labeler,
                labeling_run_id: "fixture-label-run",
                guideline_version: GOAL_RELEVANCE_GUIDELINE_VERSION,
            },
        )
        .expect("label fixture response parses");
        let labels =
            parse_label_interchange(&write_jsonl(&[label]).expect("serialize label"), &roster())
                .expect("label fixture validates through shared interchange validator");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].labeler_id, expected_labeler);
        assert_eq!(labels[0].per_goal.len(), roster().goals.len());
    }
}

#[test]
fn prompt_uses_only_goal_description_and_vague_mode_has_no_goal() {
    let description = GoalDescription {
        title: "A title".to_string(),
        summary: "A summary".to_string(),
        tension_summaries: vec!["A tension".to_string()],
    };
    let prompt = build_prompt(&PromptRequest {
        description: Some(description),
        mode: GenerationMode::Natural,
        count: 2,
        prior_cluster_anchors: Vec::new(),
        approved_cluster_anchor: None,
    })
    .expect("description prompt");
    assert!(
        prompt.contains("A title") && prompt.contains("A summary") && prompt.contains("A tension")
    );
    assert!(prompt.contains("must never address the AI as though it were a human interlocutor"));
    assert!(
        prompt.contains("vary actors, settings, speech acts, stances, tenses, and consequences")
    );
    assert!(!prompt.contains("activation keyword"));
    let vague_prompt = build_prompt(&PromptRequest {
        description: None,
        mode: GenerationMode::VagueNoneOfRoster,
        count: 2,
        prior_cluster_anchors: Vec::new(),
        approved_cluster_anchor: None,
    })
    .expect("vague prompt");
    for required in [
        "outside every roster goal",
        "requests to stop or limit a recurring conversation topic",
        "recognizing or correcting a conclusion drawn from ambiguous evidence",
        "contrasting a statement or appearance with what was actually true or intended",
        "speaker's own work, projects, manager, or deadlines",
        "remember, remembers, remembered, remembering, remind, reminds, reminded, reminder, reminders",
        "absent third parties' relationships, breakups, or affairs, even without endorsing them",
        "weighing what someone said against interpretations of what they really meant",
        "prices, the economy, technology adoption, or world trends",
    ] {
        assert!(
            vague_prompt.contains(required),
            "vague prompt omits {required}"
        );
    }
    let subject_prompt = build_prompt(&PromptRequest {
        description: Some(GoalDescription {
            title: "A title".to_string(),
            summary: "A summary".to_string(),
            tension_summaries: vec!["A tension".to_string()],
        }),
        mode: GenerationMode::SubjectConfusion,
        count: 2,
        prior_cluster_anchors: Vec::new(),
        approved_cluster_anchor: None,
    })
    .expect("subject-confusion prompt");
    for required in [
        "exactly one of these patterns",
        "the AI's own earlier question",
        "someone was reluctant so I stopped",
    ] {
        assert!(
            subject_prompt.contains(required),
            "subject prompt omits {required}"
        );
    }
}

#[test]
fn cluster_directives_are_pairwise_distinct_across_partitions_and_prompts_exclude_prior_anchors() {
    let cluster_ids = [
        "validation-cluster-1",
        "validation-cluster-2",
        "validation-cluster-3",
        "validation-cluster-4",
        "validation-hard-cluster",
        "test-cluster-1",
        "test-cluster-2",
        "test-cluster-3",
        "test-cluster-4",
        "test-hard-cluster",
    ];
    let directives = cluster_ids.map(|cluster_id| {
        *cluster_scenario_directive(cluster_id).expect("directive for every configured cluster")
    });
    for (left_index, left) in directives.iter().enumerate() {
        for right in directives.iter().skip(left_index + 1) {
            assert_ne!(left.stance, right.stance, "stance repeats");
            assert_ne!(
                left.speaker_role, right.speaker_role,
                "speaker role repeats"
            );
            assert_ne!(left.action, right.action, "action repeats");
            assert_ne!(left.consequence, right.consequence, "consequence repeats");
        }
    }
    assert!(cluster_scenario_directive("validation-cluster-5").is_none());
    assert!(cluster_scenario_directive("other-cluster-1").is_none());

    let prior_anchor = GenerationClusterAnchor {
        cluster_id: "validation-cluster-1".to_string(),
        anchor: "The user establishes an attendance limit before a meeting.".to_string(),
    };
    let cluster_id = "validation-cluster-2".to_string();
    let directive = cluster_scenario_directive(&cluster_id).expect("cluster directive");
    let prompt = build_prompt(&PromptRequest {
        description: Some(GoalDescription {
            title: "A title".to_string(),
            summary: "A summary".to_string(),
            tension_summaries: vec!["A tension".to_string()],
        }),
        mode: GenerationMode::ParaphraseCluster { cluster_id },
        count: 2,
        prior_cluster_anchors: vec![prior_anchor.clone()],
        approved_cluster_anchor: None,
    })
    .expect("cluster prompt");
    for required in [
        directive.stance,
        directive.speaker_role,
        directive.action,
        directive.consequence,
        prior_anchor.cluster_id.as_str(),
        prior_anchor.anchor.as_str(),
        "Your anchor must differ in stance, actors, action, and consequence",
    ] {
        assert!(prompt.contains(required), "prompt omits {required}");
    }
}

#[test]
fn synthetic_asr_corruption_is_seeded_and_uses_observed_modes() {
    let corrupted =
        synthetic_asr_corrupt("Nvidia, OpenAI! Plans?", 0).expect("named entities can be mangled");
    assert_eq!(corrupted, "nvidia ope nai plans");
    assert_eq!(
        corrupted,
        synthetic_asr_corrupt("Nvidia, OpenAI! Plans?", 0).expect("same named entities mangle")
    );
    assert_ne!(
        corrupted,
        synthetic_asr_corrupt("Nvidia, OpenAI! Plans?", 1)
            .expect("different seed still mangles named entities")
    );
    assert_ne!(corrupted, punctuation_casing_loss("Nvidia, OpenAI! Plans?"));
}

#[test]
fn synthetic_asr_rejects_utterances_without_a_mangleable_entity() {
    for utterance in [
        "please keep this to the agenda",
        "Honestly the meeting ran long",
    ] {
        let error = synthetic_asr_corrupt(utterance, 0).expect_err(
            "synthetic ASR must not count a sentence-opening framing word as an entity",
        );
        assert!(error.contains("mangle-able personal or product/company name"));
    }

    let corrupted = synthetic_asr_corrupt("Suppose Alex—stays late", 0)
        .expect("the mid-sentence name is a confident entity");
    assert!(corrupted.starts_with("suppose "), "{corrupted}");
    assert!(corrupted.ends_with(" stays late"), "{corrupted}");
    assert_ne!(corrupted, "suppose alex stays late");

    let parse_error = parse_generation_response(
        r#"{"utterances":["please keep this to the agenda"]}"#,
        &GenerationResponseContext {
            utterance_id_prefix: "asr-no-entity".to_string(),
            language: "en".to_string(),
            conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
            mode: GenerationMode::SyntheticAsr,
            session_id: "asr-session".to_string(),
            semantic_cluster_id: "asr-cluster".to_string(),
            generation_run_id: "asr-run".to_string(),
            generator_model_id: "gpt-5.4-nano".to_string(),
            synthetic_asr_seed: 0,
        },
    )
    .expect_err("mode parser names the source utterance for a missing entity");
    assert!(parse_error.contains("synthetic_asr"), "{parse_error}");
    assert!(parse_error.contains("asr-no-entity-01"), "{parse_error}");
}

#[test]
fn punctuation_casing_loss_mode_applies_its_transform() {
    let generated = parse_generation_response(
        r#"{"utterances":["Please, Keep THIS Private!"]}"#,
        &GenerationResponseContext {
            utterance_id_prefix: "punctuation".to_string(),
            language: "en".to_string(),
            conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
            mode: GenerationMode::PunctuationCasingLoss,
            session_id: "punctuation-session".to_string(),
            semantic_cluster_id: "punctuation-cluster".to_string(),
            generation_run_id: "punctuation-run".to_string(),
            generator_model_id: "gpt-5.4-nano".to_string(),
            synthetic_asr_seed: 0,
        },
    )
    .expect("punctuation/casing response parses");
    assert_eq!(generated[0].utterance, "please keep this private");
    assert_eq!(
        punctuation_casing_loss("Ava’s Plan—Now!"),
        "avas plan now",
        "Unicode apostrophes fuse naturally while an em dash splits words"
    );
}

#[test]
fn generation_mode_validators_accept_required_forms() {
    let context = |mode| GenerationResponseContext {
        utterance_id_prefix: "validator-accept".to_string(),
        language: "en".to_string(),
        conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
        mode,
        session_id: "validator-session".to_string(),
        semantic_cluster_id: "validator-cluster".to_string(),
        generation_run_id: "validator-run".to_string(),
        generator_model_id: "gpt-5.4-nano".to_string(),
        synthetic_asr_seed: 0,
    };

    parse_generation_response(
        r#"{"utterances":["I would rather let Maya set the pace."]}"#,
        &context(GenerationMode::ImplicitNegation),
    )
    .expect("implicit negation without grammatical negators is accepted");
    parse_generation_response(
        r#"{"utterances":["Imagine Maya changes the subject after Jordan pauses."]}"#,
        &context(GenerationMode::Hypothetical),
    )
    .expect("explicitly imagined event is accepted");
    parse_generation_response(
        r#"{"anchor":"Maya follows Jordan's lead.","utterances":["Maya takes Jordan's pause as a cue to stay with the agenda."]}"#,
        &context(GenerationMode::HardParaphrase {
            cluster_id: "validator-hard".to_string(),
        }),
    )
    .expect("indirect hard negative without forbidden vocabulary is accepted");
    parse_generation_response(
        r#"{"anchor":"Maya follows Jordan's lead.","utterances":["Maya treats the problem as Jordan's cue to stay with the agenda."]}"#,
        &context(GenerationMode::HardParaphrase {
            cluster_id: "validator-hard-problem".to_string(),
        }),
    )
    .expect("word-level matching does not reject problem");
    parse_generation_response(
        r#"{"utterances":["An unremembered song came on at the shop while I renewed my gym membership."]}"#,
        &GenerationResponseContext {
            utterance_id_prefix: "validator-vague-accept".to_string(),
            language: "en".to_string(),
            conditioning_goal_ref: None,
            mode: GenerationMode::VagueNoneOfRoster,
            session_id: "validator-session".to_string(),
            semantic_cluster_id: "validator-cluster".to_string(),
            generation_run_id: "validator-run".to_string(),
            generator_model_id: "gpt-5.4-nano".to_string(),
            synthetic_asr_seed: 0,
        },
    )
    .expect("word-boundary matching does not reject a larger word");
}

#[test]
fn generation_mode_validators_reject_invalid_utterances_with_mode_and_id() {
    let context = |mode| GenerationResponseContext {
        utterance_id_prefix: "validator-reject".to_string(),
        language: "en".to_string(),
        conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
        mode,
        session_id: "validator-session".to_string(),
        semantic_cluster_id: "validator-cluster".to_string(),
        generation_run_id: "validator-run".to_string(),
        generator_model_id: "gpt-5.4-nano".to_string(),
        synthetic_asr_seed: 0,
    };
    for (mode, response, expected_mode) in [
        (
            GenerationMode::ImplicitNegation,
            r#"{"utterances":["I don’t want Maya to continue."]}"#,
            "implicit_negation",
        ),
        (
            GenerationMode::Hypothetical,
            r#"{"utterances":["Maya changes the subject whenever Jordan pauses."]}"#,
            "hypothetical",
        ),
        (
            GenerationMode::Hypothetical,
            r#"{"utterances":["She was supposed to ask Mark first."]}"#,
            "hypothetical",
        ),
        (
            GenerationMode::HardParaphrase {
                cluster_id: "validator-hard".to_string(),
            },
            r#"{"anchor":"Maya follows Jordan's lead.","utterances":["Maya respects Jordan's boundary."]}"#,
            "hard_negative",
        ),
    ] {
        let error = parse_generation_response(response, &context(mode))
            .expect_err("invalid mode content is rejected loudly");
        assert!(error.contains(expected_mode), "{error}");
        assert!(error.contains("validator-reject-01"), "{error}");
    }

    for negator in ["cannot", "nothing", "nobody", "none", "neither", "nor"] {
        let response =
            format!(r#"{{"utterances":["Maya says {negator} before Jordan answers."]}}"#);
        let error =
            parse_generation_response(&response, &context(GenerationMode::ImplicitNegation))
                .expect_err("every explicit-negator variant is rejected");
        assert!(error.contains("implicit_negation"), "{negator}: {error}");
        assert!(error.contains("validator-reject-01"), "{negator}: {error}");
    }

    for forbidden in crate::generation::HARD_NEGATIVE_FORBIDDEN_WORDS {
        let response = format!(
            r#"{{"anchor":"Maya follows Jordan's lead.","utterances":["Maya said {forbidden} around Jordan."]}}"#
        );
        let error = parse_generation_response(
            &response,
            &context(GenerationMode::HardParaphrase {
                cluster_id: "validator-hard-inflection".to_string(),
            }),
        )
        .expect_err("every hard-negative forbidden inflection is rejected");
        assert!(error.contains("hard_negative"), "{forbidden}: {error}");
        assert!(
            error.contains("validator-reject-01"),
            "{forbidden}: {error}"
        );
    }

    let vague_context = GenerationResponseContext {
        utterance_id_prefix: "validator-vague-reject".to_string(),
        language: "en".to_string(),
        conditioning_goal_ref: None,
        mode: GenerationMode::VagueNoneOfRoster,
        session_id: "validator-session".to_string(),
        semantic_cluster_id: "validator-cluster".to_string(),
        generation_run_id: "validator-run".to_string(),
        generator_model_id: "gpt-5.4-nano".to_string(),
        synthetic_asr_seed: 0,
    };
    for forbidden in crate::generation::VAGUE_NONE_OF_ROSTER_RETENTION_WORDS
        .iter()
        .chain(crate::generation::HARD_NEGATIVE_FORBIDDEN_WORDS)
    {
        let response = format!(r#"{{"utterances":["Maya said {forbidden} around Jordan."]}}"#);
        let error = parse_generation_response(&response, &vague_context)
            .expect_err("every mode-14 forbidden word is rejected");
        assert!(error.contains("none_of_roster"), "{forbidden}: {error}");
        assert!(
            error.contains("validator-vague-reject-01"),
            "{forbidden}: {error}"
        );
    }
}

#[test]
fn cluster_modes_require_an_anchor_without_changing_the_interchange() {
    let context = GenerationResponseContext {
        utterance_id_prefix: "anchor".to_string(),
        language: "en".to_string(),
        conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
        mode: GenerationMode::ParaphraseCluster {
            cluster_id: "anchor-cluster".to_string(),
        },
        session_id: "anchor-session".to_string(),
        semantic_cluster_id: "anchor-cluster".to_string(),
        generation_run_id: "anchor-run".to_string(),
        generator_model_id: "gpt-5.4-nano".to_string(),
        synthetic_asr_seed: 0,
    };
    let error = parse_generation_response(
        r#"{"utterances":["Maya follows Jordan's lead."]}"#,
        &context,
    )
    .expect_err("cluster response without anchor is rejected");
    assert!(error.contains("paraphrase_cluster"), "{error}");
    assert!(error.contains("anchor"), "{error}");
}

#[test]
fn model_generation_response_rejects_unknown_fields_with_or_without_anchor() {
    let natural_context = GenerationResponseContext {
        utterance_id_prefix: "unknown-natural".to_string(),
        language: "en".to_string(),
        conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
        mode: GenerationMode::Natural,
        session_id: "unknown-session".to_string(),
        semantic_cluster_id: "unknown-cluster".to_string(),
        generation_run_id: "unknown-run".to_string(),
        generator_model_id: "gpt-5.4-nano".to_string(),
        synthetic_asr_seed: 0,
    };
    let cluster_context = GenerationResponseContext {
        mode: GenerationMode::ParaphraseCluster {
            cluster_id: "unknown-cluster".to_string(),
        },
        ..natural_context.clone()
    };
    for (response, context) in [
        (
            r#"{"utterances":["Maya follows Jordan's lead."],"unexpected":true}"#,
            natural_context,
        ),
        (
            r#"{"anchor":"Maya follows Jordan's lead.","utterances":["Maya follows Jordan's lead."],"unexpected":true}"#,
            cluster_context,
        ),
    ] {
        let error = parse_generation_response(response, &context)
            .expect_err("model response must reject unknown fields");
        assert!(error.contains("unknown field"), "{error}");
    }
}

#[test]
fn generation_report_lists_utterances_with_shared_metadata_stated_once() {
    let generated = parse_generation_response(
        r#"{"utterances":["First thing I said.","Second thing I said."]}"#,
        &GenerationResponseContext {
            utterance_id_prefix: "report".to_string(),
            language: "en".to_string(),
            conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
            mode: GenerationMode::QuotedSpeech,
            session_id: "report-session".to_string(),
            semantic_cluster_id: "report-cluster".to_string(),
            generation_run_id: "report-run".to_string(),
            generator_model_id: "gpt-5.4-nano".to_string(),
            synthetic_asr_seed: 0,
        },
    )
    .expect("report fixture parses");
    let report = render_generation_report(Some("Respect a person's boundaries"), &generated)
        .expect("report renders");
    assert!(report.starts_with("generated 2 utterance(s) — model gpt-5.4-nano, run report-run\n"));
    assert!(report.contains("goal: Respect a person's boundaries\n"));
    assert!(report.contains("saw_activation_keywords=false"));
    assert!(report.contains("  1. First thing I said.\n"));
    assert!(report.contains("  2. Second thing I said.\n"));
    assert!(report.contains("slices: quoted_speech"));
    assert_eq!(report.matches("report-session").count(), 1);
    assert!(render_generation_report(None, &[]).is_err());
}

#[test]
fn hard_paraphrase_is_tagged_and_limited_against_the_base_pool() {
    let mut pool = Vec::new();
    for index in 0..8 {
        let mut record = generated();
        record.utterance_id = format!("base-{index}");
        record.intended_slice_tags.clear();
        pool.push(record);
    }
    let hard = parse_generation_response(
        r#"{"anchor":"A difficult shared proposition.","utterances":["A difficult paraphrase.","Another difficult paraphrase."]}"#,
        &GenerationResponseContext {
            utterance_id_prefix: "hard".to_string(),
            language: "en".to_string(),
            conditioning_goal_ref: Some(roster().goals[0].goal_ref.clone()),
            mode: GenerationMode::HardParaphrase {
                cluster_id: "hard-cluster".to_string(),
            },
            session_id: "hard-session".to_string(),
            semantic_cluster_id: "hard-cluster".to_string(),
            generation_run_id: "hard-run".to_string(),
            generator_model_id: "gpt-5.4-nano".to_string(),
            synthetic_asr_seed: 0,
        },
    )
    .expect("hard-paraphrase response parses");
    assert!(hard.iter().all(|record| {
        record
            .intended_slice_tags
            .contains(&qsf_semantic_eval::SliceTag::HardNegative)
    }));
    pool.extend(hard);
    assert_eq!(hard_negative_count(&pool), 2);
    assert!(hard_negative_within_distribution(&pool));
}

#[test]
fn generation_validator_enforces_hard_negative_distribution_cap() {
    let mut pool = Vec::new();
    for index in 0..3 {
        let mut record = generated();
        record.utterance_id = format!("base-{index}");
        pool.push(record);
    }
    let mut hard = generated();
    hard.utterance_id = "hard".to_string();
    hard.intended_slice_tags = vec![qsf_semantic_eval::SliceTag::HardNegative];
    pool.push(hard);

    let error = validate_generation_output(&pool).expect_err("one hard to three base exceeds cap");
    assert!(error.contains("hard_negative"), "{error}");
}

#[test]
fn generation_validator_rejects_paraphrase_cluster_spanning_semantic_clusters() {
    let mut left = generated();
    left.utterance_id = "left".to_string();
    left.semantic_cluster_id = "semantic-left".to_string();
    left.intended_slice_tags = vec![qsf_semantic_eval::SliceTag::ParaphraseCluster {
        id: "shared-paraphrase".to_string(),
    }];
    let mut right = left.clone();
    right.utterance_id = "right".to_string();
    right.semantic_cluster_id = "semantic-right".to_string();

    let error = validate_generation_output(&[left, right])
        .expect_err("paraphrase cluster cannot span semantic clusters");
    assert!(error.contains("shared-paraphrase"), "{error}");
    assert!(error.contains("semantic_cluster_id"), "{error}");
}

fn feasibility_pool() -> Vec<GenerationOutput> {
    let mut pool = Vec::new();
    for partition in ["left", "right"] {
        for index in 0..8 {
            let mut record = generated();
            record.utterance_id = format!("{partition}-{index}");
            record.session_id = format!("session-{partition}");
            record.semantic_cluster_id = format!("semantic-{partition}");
            record.conditioning_goal_ref = None;
            record.intended_slice_tags = vec![
                qsf_semantic_eval::SliceTag::ExplicitNegation,
                qsf_semantic_eval::SliceTag::ImplicitNegation,
                qsf_semantic_eval::SliceTag::QuotedSpeech,
                qsf_semantic_eval::SliceTag::Hypothetical,
                qsf_semantic_eval::SliceTag::SubjectConfusion,
                qsf_semantic_eval::SliceTag::PunctuationCasingLoss,
                qsf_semantic_eval::SliceTag::SyntheticAsr,
                qsf_semantic_eval::SliceTag::RareHighCost,
                qsf_semantic_eval::SliceTag::HardNegative,
                qsf_semantic_eval::SliceTag::ParaphraseCluster {
                    id: format!("{partition}-{}", index / 2),
                },
            ];
            pool.push(record);
        }
    }
    pool
}

#[test]
fn split_feasibility_assigns_whole_components_and_names_an_unmet_floor() {
    let pool = feasibility_pool();
    let result = split_feasibility_preflight(&pool, 7).expect("fixture pool is feasible");
    assert_eq!(result.assignment_by_component.len(), 2);

    let mut infeasible = pool;
    for record in &mut infeasible {
        if record
            .intended_slice_tags
            .contains(&qsf_semantic_eval::SliceTag::ExplicitNegation)
        {
            record.session_id = "one-negation-component".to_string();
            record.semantic_cluster_id = "one-negation-component".to_string();
        }
    }
    let error =
        split_feasibility_preflight(&infeasible, 7).expect_err("negations cannot cross the split");
    assert!(
        error.contains("negation requires 6 utterances per split"),
        "{error}"
    );
}

fn realistic_infeasible_quoted_speech_pool() -> Vec<GenerationOutput> {
    let mut pool = Vec::new();
    for component in 0..25 {
        let utterance_count = if component == 0 { 12 } else { 3 };
        for utterance in 0..utterance_count {
            let mut record = generated();
            record.utterance_id = format!("realistic-{component}-{utterance}");
            record.session_id = format!("realistic-session-{component}");
            record.semantic_cluster_id = format!("realistic-semantic-{component}");
            record.conditioning_goal_ref = None;
            record.intended_slice_tags = vec![
                qsf_semantic_eval::SliceTag::ExplicitNegation,
                qsf_semantic_eval::SliceTag::ImplicitNegation,
                qsf_semantic_eval::SliceTag::Hypothetical,
                qsf_semantic_eval::SliceTag::SubjectConfusion,
                qsf_semantic_eval::SliceTag::PunctuationCasingLoss,
                qsf_semantic_eval::SliceTag::SyntheticAsr,
                qsf_semantic_eval::SliceTag::RareHighCost,
                qsf_semantic_eval::SliceTag::ParaphraseCluster {
                    id: format!("realistic-paraphrase-{component}"),
                },
            ];
            if component == 0 {
                record
                    .intended_slice_tags
                    .push(qsf_semantic_eval::SliceTag::QuotedSpeech);
            }
            if (1..=8).contains(&component) && utterance == 0 {
                record
                    .intended_slice_tags
                    .push(qsf_semantic_eval::SliceTag::HardNegative);
            }
            pool.push(record);
        }
    }
    pool
}

#[test]
fn realistic_infeasible_pool_is_fast_and_names_nonfirst_floor() {
    let pool = realistic_infeasible_quoted_speech_pool();
    assert_eq!(pool.len(), 84);
    let started = Instant::now();
    let error = split_feasibility_preflight(&pool, 7)
        .expect_err("quoted speech is confined to one component");
    assert!(
        started.elapsed().as_secs_f64() < 2.0,
        "took too long: {error}"
    );
    assert!(
        error.contains("quoted_speech requires 5 utterances per split"),
        "{error}"
    );
    assert!(!error.contains("negation requires"), "{error}");
}

#[test]
fn tool_local_price_table_is_hashed_and_covers_both_live_models() {
    let table = tool_local_price_table().expect("checked-in price table is valid");
    assert_eq!(table.version, "goalrel-generation-prices-v1");
    let mini = table
        .entries
        .iter()
        .find(|entry| entry.model_id == MINI_LABELER_ID)
        .expect("mini price entry");
    assert_eq!(mini.input_usd_per_million, 0.75);
    assert_eq!(mini.cached_input_usd_per_million, 0.075);
    assert_eq!(mini.output_usd_per_million, 4.5);
    assert!(
        crate::render_usage_report(
            "unknown-model",
            crate::TokenUsage {
                input_tokens: 10,
                cached_input_tokens: 0,
                output_tokens: 2,
            }
        )
        .contains("estimated cost unavailable")
    );
}

/// Scripted transport double: serves each queued response once, then repeats the
/// last one; counts every completion call.
struct ScriptedTransport {
    responses: std::cell::RefCell<Vec<String>>,
    calls: std::cell::Cell<usize>,
}

impl ScriptedTransport {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses: std::cell::RefCell::new(responses),
            calls: std::cell::Cell::new(0),
        }
    }
}

impl ModelTransport for ScriptedTransport {
    fn complete(&self, _request: &CompletionRequest) -> Result<crate::Completion, String> {
        self.calls.set(self.calls.get() + 1);
        let mut responses = self.responses.borrow_mut();
        let content = if responses.len() > 1 {
            responses.remove(0)
        } else {
            responses[0].clone()
        };
        Ok(crate::Completion {
            content,
            usage: None,
        })
    }
}

#[test]
fn generation_retries_a_rejected_batch_and_then_succeeds() {
    let good = ReplayTransport::default_response(FixtureResponse::Generation)
        .expect("generation fixture readable");
    let transport = ScriptedTransport::new(vec!["not json".to_string(), good]);
    let run = run_generation(&transport, &roster(), "retry-run")
        .expect("run succeeds after one batch retry");
    assert_eq!(run.records.len(), 248);
    // 30 batches plus exactly one retried first batch.
    assert_eq!(transport.calls.get(), 31);
}

#[test]
fn generation_fails_loudly_after_exhausting_batch_attempts() {
    let transport = ScriptedTransport::new(vec!["not json".to_string()]);
    let error = run_generation(&transport, &roster(), "retry-run")
        .expect_err("persistently invalid batch fails the run");
    assert!(
        error.contains("invalid generation model response"),
        "{error}"
    );
    assert_eq!(transport.calls.get(), crate::MAX_GENERATION_BATCH_ATTEMPTS);
}

#[test]
fn generation_retries_and_fails_loudly_on_persistent_under_delivery() {
    let fixture = ReplayTransport::default_response(FixtureResponse::Generation)
        .expect("generation fixture readable");
    let mut response = serde_json::from_str::<Value>(&fixture).expect("generation fixture JSON");
    response["utterances"]
        .as_array_mut()
        .expect("fixture utterances array")
        .truncate(7);
    let transport = ScriptedTransport::new(vec![response.to_string()]);

    let error = run_generation(&transport, &roster(), "under-delivery-run")
        .expect_err("persistently short batch fails the run");

    for required in ["mode natural", "delivered 7", "requested 8"] {
        assert!(error.contains(required), "error omits {required}: {error}");
    }
    assert_eq!(transport.calls.get(), crate::MAX_GENERATION_BATCH_ATTEMPTS);
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
