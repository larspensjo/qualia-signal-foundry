use std::collections::{BTreeMap, BTreeSet};

use qsf_semantic_eval::{
    GoldLabel, PairRecord, ReviewStatus, RosterSnapshot, SliceTag, UtteranceRosterAnnotation,
};
use serde::{Deserialize, Serialize};

use crate::{
    FreezeManifest, GenerationOutput, ReviewDecision, ReviewField, ReviewValue, Split,
    connected_components, sha256, split_feasibility_preflight, write_jsonl,
};

pub const HARD_QA_SLICES: &[&str] = &["negation", "quoted_speech", "hypothetical"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewedPoolSplit {
    pub validation: Vec<PairRecord>,
    pub test: Vec<PairRecord>,
    pub assignment_by_component: BTreeMap<String, Split>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SplitSummary {
    pub split_seed: u64,
    pub assignment_by_component: BTreeMap<String, Split>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgreementEvidence {
    pub agreed_pairs: u32,
    pub total_pairs: u32,
    pub rate: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationSummary {
    pub agreed_pairs: u32,
    pub total_pairs: u32,
}

#[derive(Clone, Debug)]
pub struct GatekeeperRequest<'a> {
    pub validation: &'a [PairRecord],
    pub test: &'a [PairRecord],
    pub roster: &'a RosterSnapshot,
    pub blind_qa_decisions: &'a [ReviewDecision],
}

#[derive(Clone, Debug)]
pub struct FreezeRequest<'a> {
    pub dataset_version: &'a str,
    pub roster: &'a RosterSnapshot,
    pub split_seed: u64,
    pub frozen_at: &'a str,
    pub validation: &'a [PairRecord],
    pub test: &'a [PairRecord],
    pub generation_output_sha256: &'a str,
    pub label_mini_sha256: &'a str,
    pub label_fable_sha256: &'a str,
    pub review_decisions_sha256: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenArtifacts {
    pub validation_jsonl: String,
    pub test_jsonl: String,
    pub manifest: FreezeManifest,
}

/// Deterministically assigns whole session/semantic-cluster connected components.
pub fn split_reviewed_pool(records: &[PairRecord], seed: u64) -> Result<ReviewedPoolSplit, String> {
    let utterances = reviewed_pool_utterances(records)?;
    let feasibility = split_feasibility_preflight(&utterances, seed)?;
    let mut validation = Vec::new();
    let mut test = Vec::new();
    for record in records {
        let component = feasibility
            .component_by_utterance_id
            .get(&record.utterance_id)
            .ok_or_else(|| {
                format!(
                    "split rule: missing component for utterance_id {}",
                    record.utterance_id
                )
            })?;
        match feasibility.assignment_by_component.get(component) {
            Some(Split::Validation) => validation.push(record.clone()),
            Some(Split::Test) => test.push(record.clone()),
            None => {
                return Err(format!(
                    "split rule: missing assignment for component {component}"
                ));
            }
        }
    }
    canonicalize_pairs(&mut validation);
    canonicalize_pairs(&mut test);
    Ok(ReviewedPoolSplit {
        validation,
        test,
        assignment_by_component: feasibility.assignment_by_component,
    })
}

/// Checks all conditions that make a reviewed pool eligible for a frozen set.
pub fn gatekeep_goal_relevance_freeze(request: GatekeeperRequest<'_>) -> Result<(), String> {
    request
        .roster
        .validate()
        .map_err(|error| format!("roster binding rule: {error}"))?;
    request
        .roster
        .assert_matches_current_realtime_seed()
        .map_err(|error| format!("roster round-trip rule: {error}"))?;
    let all = request
        .validation
        .iter()
        .chain(request.test.iter())
        .collect::<Vec<_>>();
    if all.is_empty() {
        return Err("dense cross-product rule: frozen splits contain no pairs".to_string());
    }
    validate_dense_cross_product(
        &all,
        request.roster,
        &request.roster.roster_snapshot_version,
    )?;
    validate_no_spanning_ids(request.validation, request.test)?;
    validate_none_of_roster(&all)?;
    validate_reviewed(&all)?;
    let validation_counts = per_slice_counts(request.validation);
    let test_counts = per_slice_counts(request.test);
    for name in REQUIRED_FLOOR_NAMES {
        validate_floor(
            "validation",
            name,
            *validation_counts.get(*name).unwrap_or(&0),
        )?;
        validate_floor("test", name, *test_counts.get(*name).unwrap_or(&0))?;
    }
    let agreements = blind_qa_agreement_by_slice(&all, request.blind_qa_decisions)?;
    for slice in HARD_QA_SLICES {
        let agreement = agreements.get(*slice).ok_or_else(|| {
            format!("blind-QA agreement rule: no cold decisions cover hard slice {slice}")
        })?;
        if agreement.rate < 0.80 {
            return Err(format!(
                "blind-QA agreement rule: {slice} agreement {:.3} is below 0.80",
                agreement.rate
            ));
        }
    }
    Ok(())
}

/// Counts the sizing-table slices by distinct utterance, including cluster count.
pub fn per_slice_counts(records: &[PairRecord]) -> BTreeMap<String, u32> {
    let mut utterance_slices: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut clusters: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for record in records {
        for name in slice_names(record) {
            utterance_slices
                .entry(name)
                .or_default()
                .insert(record.utterance_id.clone());
        }
        for tag in &record.slice_tags {
            if let SliceTag::ParaphraseCluster { id } = tag {
                clusters
                    .entry(id)
                    .or_default()
                    .insert(record.utterance_id.clone());
            }
        }
    }
    let mut result = utterance_slices
        .into_iter()
        .map(|(name, ids)| (name.to_string(), ids.len() as u32))
        .collect::<BTreeMap<_, _>>();
    result.insert(
        "paraphrase_clusters".to_string(),
        clusters.values().filter(|ids| ids.len() >= 2).count() as u32,
    );
    result
}

pub fn blind_qa_agreement_by_slice(
    records: &[&PairRecord],
    decisions: &[ReviewDecision],
) -> Result<BTreeMap<String, AgreementEvidence>, String> {
    let pair_by_key = records
        .iter()
        .map(|record| {
            (
                (record.utterance_id.as_str(), record.goal_ref.as_str()),
                *record,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let final_decision_by_pair = decisions
        .iter()
        .filter_map(
            |decision| match (&decision.field, &decision.goal_ref, &decision.value) {
                (ReviewField::GoldLabel, Some(goal_ref), ReviewValue::GoldLabel(_)) => Some((
                    (decision.utterance_id.as_str(), goal_ref.as_str()),
                    decision,
                )),
                _ => None,
            },
        )
        .collect::<BTreeMap<_, _>>();
    let mut counts: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for decision in final_decision_by_pair.into_values() {
        let (goal_ref, label) = match (&decision.field, &decision.goal_ref, &decision.value) {
            (ReviewField::GoldLabel, Some(goal_ref), ReviewValue::GoldLabel(label)) => {
                (goal_ref, label)
            }
            _ => continue,
        };
        let record = pair_by_key
            .get(&(decision.utterance_id.as_str(), goal_ref.as_str()))
            .ok_or_else(|| {
                format!(
                    "blind-QA agreement rule: decision references unknown pair {} / {}",
                    decision.utterance_id, goal_ref
                )
            })?;
        for slice in slice_names(record)
            .into_iter()
            .filter(|name| HARD_QA_SLICES.contains(name))
        {
            let entry = counts.entry(slice.to_string()).or_insert((0, 0));
            entry.0 += u32::from(&record.gold_label == label);
            entry.1 += 1;
        }
    }
    Ok(counts
        .into_iter()
        .map(|(slice, (agreed_pairs, total_pairs))| {
            let rate = if total_pairs == 0 {
                0.0
            } else {
                f64::from(agreed_pairs) / f64::from(total_pairs)
            };
            (
                slice,
                AgreementEvidence {
                    agreed_pairs,
                    total_pairs,
                    rate,
                },
            )
        })
        .collect())
}

pub fn split_summary(split: &ReviewedPoolSplit, seed: u64) -> SplitSummary {
    SplitSummary {
        split_seed: seed,
        assignment_by_component: split.assignment_by_component.clone(),
    }
}

pub fn write_split_summary(summary: &SplitSummary) -> Result<String, String> {
    serde_json::to_string_pretty(summary)
        .map(|value| format!("{value}\n"))
        .map_err(|error| error.to_string())
}

pub fn parse_split_summary(input: &str) -> Result<SplitSummary, String> {
    serde_json::from_str(input).map_err(|error| format!("invalid split summary: {error}"))
}

pub fn canonical_reviewed_pool(validation: &[PairRecord], test: &[PairRecord]) -> Vec<PairRecord> {
    let mut reviewed = validation.iter().chain(test).cloned().collect::<Vec<_>>();
    canonicalize_pairs(&mut reviewed);
    reviewed
}

pub fn verify_reviewed_pool_split(
    dataset_version: &str,
    seed: u64,
    validation: &[PairRecord],
    test: &[PairRecord],
    supplied_validation_jsonl: &str,
    supplied_test_jsonl: &str,
) -> Result<(), String> {
    let reproduced = split_reviewed_pool(&canonical_reviewed_pool(validation, test), seed)
        .map_err(|error| {
            format!(
                "split reproducibility rule: dataset_version={dataset_version} seed={seed} could not reproduce split: {error}"
            )
        })?;
    for (name, reproduced_records, supplied) in [
        (
            "validation",
            reproduced.validation.as_slice(),
            supplied_validation_jsonl,
        ),
        ("test", reproduced.test.as_slice(), supplied_test_jsonl),
    ] {
        let reproduced_jsonl = write_jsonl(reproduced_records)?;
        if reproduced_jsonl.as_bytes() != supplied.as_bytes() {
            return Err(format!(
                "split reproducibility rule: dataset_version={dataset_version} seed={seed} {name} split diverged from deterministic output"
            ));
        }
    }
    Ok(())
}

pub fn freeze_artifacts(request: FreezeRequest<'_>) -> Result<FrozenArtifacts, String> {
    let mut validation = request.validation.to_vec();
    let mut test = request.test.to_vec();
    canonicalize_pairs(&mut validation);
    canonicalize_pairs(&mut test);
    let validation_jsonl = write_jsonl(&validation)?;
    let test_jsonl = write_jsonl(&test)?;
    let mut per_slice_counts_by_split = BTreeMap::new();
    per_slice_counts_by_split.insert("validation".to_string(), per_slice_counts(&validation));
    per_slice_counts_by_split.insert("test".to_string(), per_slice_counts(&test));
    Ok(FrozenArtifacts {
        manifest: FreezeManifest {
            dataset_version: request.dataset_version.to_string(),
            roster_snapshot_version: request.roster.roster_snapshot_version.clone(),
            roster_fixture_hash: request.roster.fixture_hash.clone(),
            split_seed: request.split_seed.to_string(),
            validation_sha256: sha256(&validation_jsonl),
            test_sha256: sha256(&test_jsonl),
            per_slice_counts_by_split,
            generation_output_sha256: request.generation_output_sha256.to_string(),
            label_mini_sha256: request.label_mini_sha256.to_string(),
            label_fable_sha256: request.label_fable_sha256.to_string(),
            review_decisions_sha256: request.review_decisions_sha256.to_string(),
            frozen_at: request.frozen_at.to_string(),
        },
        validation_jsonl,
        test_jsonl,
    })
}

pub fn methodology_goal_relevance(
    manifest: &FreezeManifest,
    reconciliation: &ReconciliationSummary,
    blind_qa: &BTreeMap<String, AgreementEvidence>,
) -> String {
    let rate = if reconciliation.total_pairs == 0 {
        0.0
    } else {
        f64::from(reconciliation.agreed_pairs) / f64::from(reconciliation.total_pairs)
    };
    let counts = manifest
        .per_slice_counts_by_split
        .iter()
        .map(|(split, counts)| {
            let values = counts
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("- {split}: {values}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let qa = HARD_QA_SLICES
        .iter()
        .map(|slice| match blind_qa.get(*slice) {
            Some(value) => format!(
                "- {slice}: {}/{} ({:.2}%)",
                value.agreed_pairs,
                value.total_pairs,
                value.rate * 100.0
            ),
            None => format!("- {slice}: no decisions"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# Goal relevance dataset methodology\n\nDataset version: `{}`. Split seed: `{}`.\n\nThe generator was conditioned only on frozen goal descriptions and `saw_activation_keywords=false` for every generated record. The reviewed pool uses the dense cross-product: exactly one pair for every utterance and frozen roster goal.\n\nMini/Fable agreement: {}/{} ({:.2}%). This value is recorded in the committed `reconciliation-summary.json`.\n\n## Blind-QA agreement\n\n{}\n\n## Per-slice counts\n\n{}\n\n## Freeze hashes\n\n- roster fixture: `{}`\n- validation: `{}`\n- test: `{}`\n- generation output: `{}`\n- mini labels: `{}`\n- Fable labels: `{}`\n- review decisions: `{}`\n\n## Roster rebinding\n\nThe authoritative keyword-only rebinding rule is in `evaluation/schemas/GoalRelevanceCorpusAndRoster.md`: a new snapshot version mechanically reissues goal references and rewrites the roster version; title, summary, or tension-summary changes require relabeling.\n",
        manifest.dataset_version,
        manifest.split_seed,
        reconciliation.agreed_pairs,
        reconciliation.total_pairs,
        rate * 100.0,
        qa,
        counts,
        manifest.roster_fixture_hash,
        manifest.validation_sha256,
        manifest.test_sha256,
        manifest.generation_output_sha256,
        manifest.label_mini_sha256,
        manifest.label_fable_sha256,
        manifest.review_decisions_sha256
    )
}

pub fn reconciliation_summary(records: &[crate::ReconciliationRecord]) -> ReconciliationSummary {
    ReconciliationSummary {
        agreed_pairs: records.iter().filter(|record| record.agree).count() as u32,
        total_pairs: records.len() as u32,
    }
}

fn validate_dense_cross_product(
    records: &[&PairRecord],
    roster: &RosterSnapshot,
    version: &str,
) -> Result<(), String> {
    let expected = roster
        .goals
        .iter()
        .map(|goal| goal.goal_ref.as_str())
        .collect::<BTreeSet<_>>();
    let mut by_utterance: BTreeMap<&str, Vec<&PairRecord>> = BTreeMap::new();
    for record in records {
        if record.roster_snapshot_version != version {
            return Err(format!(
                "roster binding rule: utterance_id {} has roster_snapshot_version {} instead of {version}",
                record.utterance_id, record.roster_snapshot_version
            ));
        }
        if !expected.contains(record.goal_ref.as_str()) {
            return Err(format!(
                "roster binding rule: goal_ref {} does not resolve in the frozen roster",
                record.goal_ref
            ));
        }
        by_utterance
            .entry(&record.utterance_id)
            .or_default()
            .push(*record);
    }
    for (utterance_id, pairs) in by_utterance {
        let actual = pairs
            .iter()
            .map(|pair| pair.goal_ref.as_str())
            .collect::<BTreeSet<_>>();
        if pairs.len() != expected.len() || actual != expected {
            return Err(format!(
                "dense cross-product rule: utterance_id {utterance_id} must have exactly one pair for every roster goal"
            ));
        }
        let first = pairs[0];
        for pair in &pairs[1..] {
            if pair.utterance != first.utterance
                || pair.session_id != first.session_id
                || pair.semantic_cluster_id != first.semantic_cluster_id
                || pair.language != first.language
                || pair.utterance_roster_annotation != first.utterance_roster_annotation
                || pair.slice_tags != first.slice_tags
            {
                return Err(format!(
                    "dense cross-product rule: utterance_id {utterance_id} has inconsistent per-utterance metadata"
                ));
            }
        }
    }
    Ok(())
}

fn validate_no_spanning_ids(validation: &[PairRecord], test: &[PairRecord]) -> Result<(), String> {
    for (rule, validation_ids, test_ids) in [
        (
            "session_id",
            validation
                .iter()
                .map(|pair| &pair.session_id)
                .collect::<BTreeSet<_>>(),
            test.iter()
                .map(|pair| &pair.session_id)
                .collect::<BTreeSet<_>>(),
        ),
        (
            "semantic_cluster_id",
            validation
                .iter()
                .map(|pair| &pair.semantic_cluster_id)
                .collect::<BTreeSet<_>>(),
            test.iter()
                .map(|pair| &pair.semantic_cluster_id)
                .collect::<BTreeSet<_>>(),
        ),
    ] {
        if let Some(value) = validation_ids.intersection(&test_ids).next() {
            return Err(format!(
                "split integrity rule: {rule} {value} appears in both validation and test"
            ));
        }
    }
    Ok(())
}

fn validate_none_of_roster(records: &[&PairRecord]) -> Result<(), String> {
    for record in records {
        if record.utterance_roster_annotation == UtteranceRosterAnnotation::NoneOfRoster
            && record.gold_label == GoldLabel::Relevant
        {
            return Err(format!(
                "none_of_roster rule: utterance_id {} has a relevant pair",
                record.utterance_id
            ));
        }
    }
    Ok(())
}

fn validate_reviewed(records: &[&PairRecord]) -> Result<(), String> {
    if let Some(record) = records
        .iter()
        .find(|record| record.provenance.review.review_status != ReviewStatus::Reviewed)
    {
        return Err(format!(
            "review completeness rule: utterance_id {} goal_ref {} is not reviewed",
            record.utterance_id, record.goal_ref
        ));
    }
    Ok(())
}

fn validate_floor(split: &str, name: &str, count: u32) -> Result<(), String> {
    let floor = match name {
        "negation" => 6,
        "explicit_negation" | "implicit_negation" => 2,
        "quoted_speech" | "hypothetical" => 5,
        "subject_confusion" | "punctuation_casing_loss" | "synthetic_asr" | "rare_high_cost" => 3,
        "hard_negative" => 4,
        "none_of_roster" => 8,
        "paraphrase_clusters" => 4,
        _ => return Ok(()),
    };
    if count < floor {
        return Err(format!(
            "per-slice floor rule: {split} {name} has {count}, requires {floor}"
        ));
    }
    Ok(())
}

const REQUIRED_FLOOR_NAMES: &[&str] = &[
    "negation",
    "explicit_negation",
    "implicit_negation",
    "quoted_speech",
    "hypothetical",
    "subject_confusion",
    "punctuation_casing_loss",
    "synthetic_asr",
    "rare_high_cost",
    "hard_negative",
    "none_of_roster",
    "paraphrase_clusters",
];

fn slice_names(record: &PairRecord) -> BTreeSet<&'static str> {
    let mut names = BTreeSet::new();
    if record.utterance_roster_annotation == UtteranceRosterAnnotation::NoneOfRoster {
        names.insert("none_of_roster");
    }
    for tag in &record.slice_tags {
        match tag {
            SliceTag::ExplicitNegation => {
                names.insert("negation");
                names.insert("explicit_negation");
            }
            SliceTag::ImplicitNegation => {
                names.insert("negation");
                names.insert("implicit_negation");
            }
            SliceTag::QuotedSpeech => {
                names.insert("quoted_speech");
            }
            SliceTag::Hypothetical => {
                names.insert("hypothetical");
            }
            SliceTag::SubjectConfusion => {
                names.insert("subject_confusion");
            }
            SliceTag::PunctuationCasingLoss => {
                names.insert("punctuation_casing_loss");
            }
            SliceTag::SyntheticAsr => {
                names.insert("synthetic_asr");
            }
            SliceTag::RareHighCost => {
                names.insert("rare_high_cost");
            }
            SliceTag::HardNegative => {
                names.insert("hard_negative");
            }
            SliceTag::ParaphraseCluster { .. } | SliceTag::RealAsr => {}
        }
    }
    names
}

fn reviewed_pool_utterances(records: &[PairRecord]) -> Result<Vec<GenerationOutput>, String> {
    let mut by_id = BTreeMap::new();
    for record in records {
        by_id
            .entry(&record.utterance_id)
            .or_insert_with(|| GenerationOutput {
                interchange_version: crate::INTERCHANGE_VERSION,
                utterance_id: record.utterance_id.clone(),
                utterance: record.utterance.clone(),
                language: record.language.clone(),
                conditioning_goal_ref: (record.utterance_roster_annotation
                    != UtteranceRosterAnnotation::NoneOfRoster)
                    .then(|| "reviewed-pool".to_string()),
                intended_slice_tags: record.slice_tags.clone(),
                session_id: record.session_id.clone(),
                semantic_cluster_id: record.semantic_cluster_id.clone(),
                generation_run_id: "reviewed-pool".to_string(),
                generator_model_id: "reviewed-pool".to_string(),
                prompt_version: "reviewed-pool".to_string(),
                saw_activation_keywords: false,
            });
    }
    let utterances = by_id.into_values().collect::<Vec<_>>();
    connected_components(&utterances)?;
    Ok(utterances)
}

fn canonicalize_pairs(records: &mut [PairRecord]) {
    records.sort_by(|left, right| {
        left.utterance_id
            .cmp(&right.utterance_id)
            .then_with(|| left.goal_ref.cmp(&right.goal_ref))
    });
}
