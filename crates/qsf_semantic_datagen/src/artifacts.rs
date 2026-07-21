use std::collections::{BTreeMap, BTreeSet};

use qsf_semantic_eval::{
    DATASET_SCHEMA_VERSION, DataSource, FrozenGoalRef, GenerationLineage, GoldLabel,
    LabelerLineage, LabelingLineage, PairRecord, ReviewLineage, ReviewStatus, RosterSnapshot,
    SliceTag, UtteranceRosterAnnotation,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INTERCHANGE_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationOutput {
    pub interchange_version: u16,
    pub utterance_id: String,
    pub utterance: String,
    pub language: String,
    pub conditioning_goal_ref: Option<String>,
    pub intended_slice_tags: Vec<SliceTag>,
    pub session_id: String,
    pub semantic_cluster_id: String,
    pub generation_run_id: String,
    pub generator_model_id: String,
    pub prompt_version: String,
    pub saw_activation_keywords: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LabelingInput {
    pub interchange_version: u16,
    pub utterance_id: String,
    pub utterance: String,
    pub language: String,
    pub roster_snapshot_version: String,
    pub roster: Vec<FrozenGoalRef>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerGoalLabel {
    pub goal_ref: String,
    pub label: GoldLabel,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LabelInterchange {
    pub interchange_version: u16,
    pub labeler_id: String,
    pub labeling_run_id: String,
    pub guideline_version: String,
    pub utterance_id: String,
    pub per_goal: Vec<PerGoalLabel>,
    pub none_of_roster: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationRecord {
    pub utterance_id: String,
    pub goal_ref: String,
    pub mini_label: GoldLabel,
    pub fable_label: GoldLabel,
    pub agree: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewField {
    GoldLabel,
    NoneOfRoster,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ReviewValue {
    GoldLabel(GoldLabel),
    NoneOfRoster(bool),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecision {
    pub decided_at: String,
    pub utterance_id: String,
    pub goal_ref: Option<String>,
    pub field: ReviewField,
    pub value: ReviewValue,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FreezeManifest {
    pub dataset_version: String,
    pub roster_snapshot_version: String,
    pub roster_fixture_hash: String,
    pub split_seed: String,
    pub validation_sha256: String,
    pub test_sha256: String,
    pub per_slice_counts_by_split: BTreeMap<String, BTreeMap<String, u32>>,
    pub generation_output_sha256: String,
    pub label_mini_sha256: String,
    pub label_fable_sha256: String,
    pub review_decisions_sha256: String,
    pub frozen_at: String,
}

pub fn parse_jsonl<T: for<'a> Deserialize<'a>>(
    input: &str,
    artifact: &str,
) -> Result<Vec<T>, String> {
    input
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| {
                format!("invalid {artifact} record at line {}: {error}", index + 1)
            })
        })
        .collect()
}

pub fn write_jsonl<T: Serialize>(records: &[T]) -> Result<String, String> {
    records
        .iter()
        .map(|record| serde_json::to_string(record).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()
        .map(|lines| {
            if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", lines.join("\n"))
            }
        })
}

pub fn parse_generation_output(input: &str) -> Result<Vec<GenerationOutput>, String> {
    let records = parse_jsonl(input, "generation-output")?;
    validate_generation_output(&records)?;
    Ok(records)
}

pub fn validate_generation_output(records: &[GenerationOutput]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for record in records {
        if record.interchange_version != INTERCHANGE_VERSION {
            return Err(format!(
                "unsupported generation interchange_version {}",
                record.interchange_version
            ));
        }
        if record.saw_activation_keywords {
            return Err(format!(
                "generation {} saw activation keywords",
                record.utterance_id
            ));
        }
        if record.utterance_id.trim().is_empty() || record.utterance.trim().is_empty() {
            return Err("generation output has an empty utterance identifier or text".to_string());
        }
        if !ids.insert(&record.utterance_id) {
            return Err(format!(
                "duplicate generated utterance_id {}",
                record.utterance_id
            ));
        }
    }
    Ok(())
}

pub fn build_labeling_input(
    generated: &[GenerationOutput],
    roster: &RosterSnapshot,
) -> Result<Vec<LabelingInput>, String> {
    validate_generation_output(generated)?;
    roster.validate()?;
    Ok(generated
        .iter()
        .map(|record| LabelingInput {
            interchange_version: INTERCHANGE_VERSION,
            utterance_id: record.utterance_id.clone(),
            utterance: record.utterance.clone(),
            language: record.language.clone(),
            roster_snapshot_version: roster.roster_snapshot_version.clone(),
            roster: roster.goals.clone(),
        })
        .collect())
}

pub fn parse_labeling_input(input: &str) -> Result<Vec<LabelingInput>, String> {
    let records: Vec<LabelingInput> = parse_jsonl(input, "labeling-input")?;
    for record in &records {
        if record.interchange_version != INTERCHANGE_VERSION {
            return Err(format!(
                "unsupported labeling input interchange_version {}",
                record.interchange_version
            ));
        }
    }
    Ok(records)
}

pub fn parse_label_interchange(
    input: &str,
    roster: &RosterSnapshot,
) -> Result<Vec<LabelInterchange>, String> {
    let records = parse_jsonl(input, "label interchange")?;
    validate_label_interchange(&records, roster)?;
    Ok(records)
}

pub fn validate_label_interchange(
    records: &[LabelInterchange],
    roster: &RosterSnapshot,
) -> Result<(), String> {
    roster.validate()?;
    let known: BTreeSet<_> = roster
        .goals
        .iter()
        .map(|goal| goal.goal_ref.as_str())
        .collect();
    let mut utterance_ids = BTreeSet::new();
    for record in records {
        if record.interchange_version != INTERCHANGE_VERSION {
            return Err(format!(
                "unsupported label interchange_version {}",
                record.interchange_version
            ));
        }
        if !utterance_ids.insert(&record.utterance_id) {
            return Err(format!(
                "duplicate label record for utterance_id {}",
                record.utterance_id
            ));
        }
        let mut labels = BTreeSet::new();
        for pair in &record.per_goal {
            if !known.contains(pair.goal_ref.as_str()) {
                return Err(format!(
                    "label goal_ref {} does not resolve to roster",
                    pair.goal_ref
                ));
            }
            if !labels.insert(&pair.goal_ref) {
                return Err(format!("duplicate label for goal_ref {}", pair.goal_ref));
            }
            if record.none_of_roster && pair.label.is_relevant() {
                return Err(format!(
                    "none_of_roster record {} has relevant goal_ref {}",
                    record.utterance_id, pair.goal_ref
                ));
            }
        }
    }
    Ok(())
}

pub fn reconcile(
    mini: &[LabelInterchange],
    fable: &[LabelInterchange],
) -> Result<Vec<ReconciliationRecord>, String> {
    let flatten =
        |records: &[LabelInterchange]| -> Result<BTreeMap<(String, String), GoldLabel>, String> {
            let mut labels = BTreeMap::new();
            for record in records {
                for pair in &record.per_goal {
                    if labels
                        .insert(
                            (record.utterance_id.clone(), pair.goal_ref.clone()),
                            pair.label.clone(),
                        )
                        .is_some()
                    {
                        return Err("duplicate reconciliation key".to_string());
                    }
                }
            }
            Ok(labels)
        };
    let mini = flatten(mini)?;
    let fable = flatten(fable)?;
    if mini.keys().collect::<Vec<_>>() != fable.keys().collect::<Vec<_>>() {
        return Err("mini and fable labels cover different utterance/goal pairs".to_string());
    }
    Ok(mini
        .into_iter()
        .map(|((utterance_id, goal_ref), mini_label)| {
            let fable_label = fable
                .get(&(utterance_id.clone(), goal_ref.clone()))
                .expect("keys checked")
                .clone();
            let agree = mini_label == fable_label;
            ReconciliationRecord {
                utterance_id,
                goal_ref,
                mini_label,
                fable_label,
                agree,
            }
        })
        .collect())
}

pub fn parse_reconciliation(input: &str) -> Result<Vec<ReconciliationRecord>, String> {
    parse_jsonl(input, "reconciliation")
}
pub fn parse_review_decisions(input: &str) -> Result<Vec<ReviewDecision>, String> {
    let records = parse_jsonl::<ReviewDecision>(input, "review decisions")?;
    for record in &records {
        validate_review_decision(record)?;
    }
    Ok(records)
}
fn validate_review_decision(record: &ReviewDecision) -> Result<(), String> {
    match (&record.field, &record.goal_ref, &record.value) {
        (ReviewField::GoldLabel, Some(_), ReviewValue::GoldLabel(_))
        | (ReviewField::NoneOfRoster, None, ReviewValue::NoneOfRoster(_)) => Ok(()),
        _ => Err(format!(
            "invalid review decision for utterance_id {}",
            record.utterance_id
        )),
    }
}

#[derive(Clone, Debug)]
pub struct ReviewedPoolRequest<'a> {
    pub generated: &'a [GenerationOutput],
    pub roster: &'a RosterSnapshot,
    pub mini: &'a [LabelInterchange],
    pub fable: &'a [LabelInterchange],
    pub decisions: &'a [ReviewDecision],
    pub dataset_version: &'a str,
    pub generation_output_sha256: &'a str,
    pub label_mini_sha256: &'a str,
    pub label_fable_sha256: &'a str,
    pub review_decisions_sha256: &'a str,
}

pub fn fold_reviewed_pool(request: ReviewedPoolRequest<'_>) -> Result<Vec<PairRecord>, String> {
    validate_generation_output(request.generated)?;
    validate_label_interchange(request.mini, request.roster)?;
    validate_label_interchange(request.fable, request.roster)?;
    let reconciled = reconcile(request.mini, request.fable)?;
    let mut labels: BTreeMap<(String, String), GoldLabel> = reconciled
        .into_iter()
        .map(|item| ((item.utterance_id, item.goal_ref), item.mini_label))
        .collect();
    let mut none: BTreeMap<String, bool> = request
        .mini
        .iter()
        .map(|item| (item.utterance_id.clone(), item.none_of_roster))
        .collect();
    let mut reviewed_labels = BTreeSet::new();
    let mut reviewed_annotations = BTreeSet::new();
    for decision in request.decisions {
        validate_review_decision(decision)?;
        match (&decision.field, &decision.goal_ref, &decision.value) {
            (ReviewField::GoldLabel, Some(goal_ref), ReviewValue::GoldLabel(label)) => {
                reviewed_labels.insert((decision.utterance_id.clone(), goal_ref.clone()));
                labels.insert(
                    (decision.utterance_id.clone(), goal_ref.clone()),
                    label.clone(),
                );
            }
            (ReviewField::NoneOfRoster, None, ReviewValue::NoneOfRoster(value)) => {
                reviewed_annotations.insert(decision.utterance_id.clone());
                none.insert(decision.utterance_id.clone(), *value);
            }
            _ => unreachable!("validated above"),
        }
    }
    let generated: BTreeMap<_, _> = request
        .generated
        .iter()
        .map(|record| (record.utterance_id.as_str(), record))
        .collect();
    let mut result = Vec::new();
    for ((utterance_id, goal_ref), gold_label) in labels {
        let generated = generated
            .get(utterance_id.as_str())
            .ok_or_else(|| format!("label references unknown utterance_id {utterance_id}"))?;
        let annotation = if *none.get(&utterance_id).unwrap_or(&false) {
            UtteranceRosterAnnotation::NoneOfRoster
        } else {
            UtteranceRosterAnnotation::HasRosterRelevance
        };
        let review_status = if reviewed_labels.contains(&(utterance_id.clone(), goal_ref.clone()))
            && reviewed_annotations.contains(&utterance_id)
        {
            ReviewStatus::Reviewed
        } else {
            ReviewStatus::Draft
        };
        result.push(PairRecord {
            schema_version: DATASET_SCHEMA_VERSION,
            dataset_version: request.dataset_version.to_string(),
            roster_snapshot_version: request.roster.roster_snapshot_version.clone(),
            utterance_id,
            utterance: generated.utterance.clone(),
            goal_ref,
            gold_label,
            slice_tags: generated.intended_slice_tags.clone(),
            language: generated.language.clone(),
            session_id: generated.session_id.clone(),
            semantic_cluster_id: generated.semantic_cluster_id.clone(),
            provenance: qsf_semantic_eval::Provenance {
                source: DataSource::Teacher,
                generation: Some(GenerationLineage {
                    generator_model_id: generated.generator_model_id.clone(),
                    generation_run_id: generated.generation_run_id.clone(),
                    generation_output_sha256: request.generation_output_sha256.to_string(),
                    prompt_version: generated.prompt_version.clone(),
                    saw_activation_keywords: generated.saw_activation_keywords,
                }),
                labeling: LabelingLineage {
                    guideline_version: request
                        .mini
                        .first()
                        .map(|item| item.guideline_version.clone())
                        .unwrap_or_default(),
                    labelers: vec![
                        LabelerLineage {
                            labeler_id: request
                                .mini
                                .first()
                                .map(|item| item.labeler_id.clone())
                                .unwrap_or_default(),
                            labeling_run_id: request
                                .mini
                                .first()
                                .map(|item| item.labeling_run_id.clone())
                                .unwrap_or_default(),
                            output_sha256: request.label_mini_sha256.to_string(),
                        },
                        LabelerLineage {
                            labeler_id: request
                                .fable
                                .first()
                                .map(|item| item.labeler_id.clone())
                                .unwrap_or_default(),
                            labeling_run_id: request
                                .fable
                                .first()
                                .map(|item| item.labeling_run_id.clone())
                                .unwrap_or_default(),
                            output_sha256: request.label_fable_sha256.to_string(),
                        },
                    ],
                },
                review: ReviewLineage {
                    review_decisions_sha256: request.review_decisions_sha256.to_string(),
                    review_status,
                },
            },
            utterance_roster_annotation: annotation,
        });
    }
    qsf_semantic_eval::Dataset {
        records: result.clone(),
    }
    .validate()?;
    Ok(result)
}

pub fn parse_reviewed_pool(input: &str) -> Result<Vec<PairRecord>, String> {
    let dataset = qsf_semantic_eval::Dataset::from_jsonl_str(input, "reviewed-pool")?;
    Ok(dataset.records)
}
pub fn write_freeze_manifest(manifest: &FreezeManifest) -> Result<String, String> {
    serde_json::to_string_pretty(manifest)
        .map(|value| format!("{value}\n"))
        .map_err(|error| error.to_string())
}
pub fn parse_freeze_manifest(input: &str) -> Result<FreezeManifest, String> {
    serde_json::from_str(input).map_err(|error| format!("invalid freeze manifest: {error}"))
}
pub fn sha256(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
