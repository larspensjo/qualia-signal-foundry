use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{Dataset, GoldLabel, RosterSnapshot, SliceTag};

pub const SCORER_SOURCE: &str =
    "qsf_volition::normalize_terms + qsf_volition::matched_keywords + qsf_volition::match_strength";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MatchedTerm {
    pub term: String,
    pub weight_class: qsf_volition::KeywordWeightClass,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairResult {
    pub dataset_version: String,
    pub roster_snapshot_hash: String,
    pub utterance: String,
    pub goal_ref: String,
    pub gold_label: GoldLabel,
    pub slice_tags: Vec<SliceTag>,
    pub matched_terms: Vec<MatchedTerm>,
    pub match_strength: u32,
    pub qualification_threshold_in_force: u32,
    pub qualifies_at_threshold_in_force: bool,
    pub scorer_source: String,
}

/// Scores every frozen pair with the production tokenization and lexical scorer.
pub fn run_baseline(
    dataset: &Dataset,
    snapshot: &RosterSnapshot,
) -> Result<Vec<PairResult>, String> {
    snapshot.validate()?;
    dataset.validate()?;
    dataset
        .records
        .iter()
        .map(|record| {
            let goal = snapshot.goal_for_ref(&record.goal_ref).ok_or_else(|| {
                format!(
                    "dataset record for utterance {:?} references goal_ref {} absent from roster {}",
                    record.utterance, record.goal_ref, snapshot.roster_snapshot_version
                )
            })?;
            let input_terms = qsf_volition::normalize_terms(&record.utterance);
            let matched_keywords = qsf_volition::matched_keywords(goal, &input_terms);
            let match_strength = qsf_volition::match_strength(&matched_keywords);
            let qualification_threshold_in_force = snapshot.fixture.arbitration_qualification_threshold;
            Ok(PairResult {
                dataset_version: record.dataset_version.clone(),
                roster_snapshot_hash: snapshot.fixture_hash.clone(),
                utterance: record.utterance.clone(),
                goal_ref: record.goal_ref.clone(),
                gold_label: record.gold_label.clone(),
                slice_tags: record.slice_tags.clone(),
                matched_terms: matched_keywords
                    .into_iter()
                    .map(|keyword| MatchedTerm {
                        term: keyword.term,
                        weight_class: keyword.weight_class,
                    })
                    .collect(),
                match_strength,
                qualification_threshold_in_force,
                qualifies_at_threshold_in_force: match_strength >= qualification_threshold_in_force,
                scorer_source: SCORER_SOURCE.to_string(),
            })
        })
        .collect()
}

pub fn default_dataset_path() -> &'static Path {
    Path::new("evaluation/frozen/goal-relevance/sample.dataset.jsonl")
}

pub fn default_roster_snapshot_path() -> &'static Path {
    Path::new("evaluation/frozen/goal-relevance/realtime-seed.roster.json")
}

pub fn load_default_inputs() -> Result<(Dataset, RosterSnapshot), String> {
    Ok((
        Dataset::from_jsonl_path(default_dataset_path())?,
        RosterSnapshot::from_json_path(default_roster_snapshot_path())?,
    ))
}

pub fn write_pair_results_jsonl(
    path: impl AsRef<Path>,
    results: &[PairResult],
) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "could not create result directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let mut output = String::new();
    for result in results {
        let line = serde_json::to_string(result).map_err(|error| {
            format!(
                "could not serialize pair result for {:?}: {error}",
                result.utterance
            )
        })?;
        output.push_str(&line);
        output.push('\n');
    }
    fs::write(path, output)
        .map_err(|error| format!("could not write pair results {}: {error}", path.display()))
}

pub fn parse_pair_results_jsonl(path: impl AsRef<Path>) -> Result<Vec<PairResult>, String> {
    let path = path.as_ref();
    let input = fs::read_to_string(path)
        .map_err(|error| format!("could not read pair results {}: {error}", path.display()))?;
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| {
                format!(
                    "invalid pair result in {} at line {}: {error}",
                    path.display(),
                    index + 1
                )
            })
        })
        .collect()
}
