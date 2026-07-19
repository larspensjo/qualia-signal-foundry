use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

pub const DATASET_SCHEMA_VERSION: u16 = 1;

fn default_language() -> String {
    "en".to_string()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoldLabel {
    Relevant,
    NotRelevant,
    Ambiguous,
}

impl GoldLabel {
    pub fn is_binary(&self) -> bool {
        !matches!(self, Self::Ambiguous)
    }

    pub fn is_relevant(&self) -> bool {
        matches!(self, Self::Relevant)
    }
}

/// An utterance-level assertion, deliberately distinct from the pair-level gold label.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UtteranceRosterAnnotation {
    HasRosterRelevance,
    NoneOfRoster,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    Teacher,
    RealSession,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Draft,
    Reviewed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub source: DataSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teacher_model_id: Option<String>,
    pub review_status: ReviewStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SliceTag {
    ParaphraseCluster { id: String },
    HardNegative,
    ExplicitNegation,
    ImplicitNegation,
    QuotedSpeech,
    Hypothetical,
    SubjectConfusion,
    PunctuationCasingLoss,
    SyntheticAsr,
    RealAsr,
    RareHighCost,
}

impl SliceTag {
    pub fn report_names(&self) -> Vec<&'static str> {
        match self {
            Self::ExplicitNegation | Self::ImplicitNegation => vec!["negation"],
            Self::QuotedSpeech => vec!["quoted_speech"],
            Self::Hypothetical => vec!["hypothetical"],
            Self::SyntheticAsr | Self::RealAsr => vec!["asr_corruption"],
            Self::HardNegative => vec!["hard_negative"],
            Self::SubjectConfusion => vec!["subject_confusion"],
            Self::PunctuationCasingLoss => vec!["punctuation_casing_loss"],
            Self::RareHighCost => vec!["rare_high_cost"],
            Self::ParaphraseCluster { .. } => Vec::new(),
        }
    }

    pub fn paraphrase_cluster_id(&self) -> Option<&str> {
        match self {
            Self::ParaphraseCluster { id } => Some(id),
            _ => None,
        }
    }
}

/// One persona-independent `(utterance, goal-description)` annotation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairRecord {
    pub schema_version: u16,
    pub dataset_version: String,
    pub utterance: String,
    pub goal_ref: String,
    pub gold_label: GoldLabel,
    #[serde(default)]
    pub slice_tags: Vec<SliceTag>,
    #[serde(default = "default_language")]
    pub language: String,
    pub session_id: String,
    pub semantic_cluster_id: String,
    pub provenance: Provenance,
    pub utterance_roster_annotation: UtteranceRosterAnnotation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dataset {
    pub records: Vec<PairRecord>,
}

impl Dataset {
    pub fn from_jsonl_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .map_err(|error| format!("could not read dataset {}: {error}", path.display()))?;
        Self::from_jsonl_str(&contents, &path.display().to_string())
    }

    pub fn from_jsonl_str(input: &str, source: &str) -> Result<Self, String> {
        let mut records = Vec::new();
        for (index, line) in input.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: PairRecord = serde_json::from_str(line).map_err(|error| {
                format!(
                    "invalid dataset record in {source} at line {}: {error}",
                    index + 1
                )
            })?;
            records.push(record);
        }
        let dataset = Self { records };
        dataset
            .validate()
            .map_err(|error| format!("invalid dataset {source}: {error}"))?;
        Ok(dataset)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.records.is_empty() {
            return Err("dataset has no pair records".to_string());
        }
        let dataset_version = &self.records[0].dataset_version;
        let mut annotations = BTreeMap::new();
        for (index, record) in self.records.iter().enumerate() {
            if record.schema_version != DATASET_SCHEMA_VERSION {
                return Err(format!(
                    "record {} uses schema_version {}; supported version is {}",
                    index + 1,
                    record.schema_version,
                    DATASET_SCHEMA_VERSION
                ));
            }
            if record.dataset_version != *dataset_version {
                return Err(format!(
                    "record {} has a different dataset_version",
                    index + 1
                ));
            }
            for (name, value) in [
                ("dataset_version", &record.dataset_version),
                ("utterance", &record.utterance),
                ("goal_ref", &record.goal_ref),
                ("language", &record.language),
                ("session_id", &record.session_id),
                ("semantic_cluster_id", &record.semantic_cluster_id),
            ] {
                if value.trim().is_empty() {
                    return Err(format!("record {} has an empty {name}", index + 1));
                }
            }
            let annotation = annotations
                .entry(&record.utterance)
                .or_insert(&record.utterance_roster_annotation);
            if *annotation != &record.utterance_roster_annotation {
                return Err(format!(
                    "utterance {:?} has inconsistent utterance-level roster annotations",
                    record.utterance
                ));
            }
            if record.utterance_roster_annotation == UtteranceRosterAnnotation::NoneOfRoster
                && record.gold_label == GoldLabel::Relevant
            {
                return Err(format!(
                    "NoneOfRoster utterance {:?} has a Relevant pair for goal_ref {}",
                    record.utterance, record.goal_ref
                ));
            }
        }
        Ok(())
    }

    pub fn dataset_version(&self) -> &str {
        &self.records[0].dataset_version
    }
}
