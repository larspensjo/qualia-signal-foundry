use std::{path::PathBuf, process::Command, time::Instant};

use qsf_semantic_eval::{GoldLabel, ReviewStatus, RosterSnapshot};
use serde_json::Value;

use crate::{
    CompletionRequest, FixtureResponse, GenerationMode, GenerationOutput,
    GenerationResponseContext, GoalDescription, INTERCHANGE_VERSION, LabelInterchange,
    ModelTransport, PerGoalLabel, PromptRequest, ReplayTransport, ReviewDecision, ReviewField,
    ReviewValue, ReviewedPoolRequest, TransportKind, build_labeling_input, build_prompt,
    default_transport_kind, fold_reviewed_pool, hard_negative_count,
    hard_negative_within_distribution, parse_generation_response, parse_label_interchange,
    punctuation_casing_loss, reconcile, render_generation_report, split_feasibility_preflight,
    synthetic_asr_corrupt, tool_local_price_table, validate_generation_output, write_jsonl,
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
        "Could automation change work?",
        "I heard the new system will decide it for us.",
        "What if the evidence changes tomorrow?",
        "Please leave that part of my story private.",
        "Nvidia and OpenAI are changing how teams plan.",
        "I want a clearer way to separate what happened from what I infer.",
        "Maybe my colleague meant a different project.",
        "This detail could cost someone their job.",
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
        let labels = parse_label_interchange(&response.content, &roster())
            .expect("label fixture parses and validates against roster");
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
    })
    .expect("description prompt");
    assert!(
        prompt.contains("A title") && prompt.contains("A summary") && prompt.contains("A tension")
    );
    assert!(!prompt.contains("activation keyword"));
    assert!(
        build_prompt(&PromptRequest {
            description: None,
            mode: GenerationMode::VagueNoneOfRoster,
            count: 2,
        })
        .is_ok()
    );
}

#[test]
fn synthetic_asr_corruption_is_seeded_and_uses_observed_modes() {
    let corrupted = synthetic_asr_corrupt("Nvidia, OpenAI! Plans?", 0);
    assert_eq!(corrupted, "nvi dia ope nai plans");
    assert_eq!(
        corrupted,
        synthetic_asr_corrupt("Nvidia, OpenAI! Plans?", 0)
    );
    assert_ne!(
        corrupted,
        synthetic_asr_corrupt("Nvidia, OpenAI! Plans?", 1)
    );
    assert_ne!(corrupted, punctuation_casing_loss("Nvidia, OpenAI! Plans?"));
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
        r#"{"utterances":["A difficult paraphrase.","Another difficult paraphrase."]}"#,
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
fn tool_local_price_table_is_hashed_and_unknown_models_are_token_only() {
    let table = tool_local_price_table().expect("checked-in price table is valid");
    assert_eq!(table.version, "goalrel-generation-prices-v1");
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
