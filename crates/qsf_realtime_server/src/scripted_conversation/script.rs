use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::realtime::volition_inspection_capture::VolitionInspectionCapture;
use crate::realtime::world_perception_capture::WorldPerceptionCapture;

pub const FIXTURE_ROOT: &str = "docs/Experiments/Fixtures/realtime-probe";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PhraseSet {
    pub id: String,
    pub description: String,
    pub phrases: Vec<Phrase>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Phrase {
    pub text: String,
    pub expected: PhraseExpected,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PhraseExpected {
    #[serde(default)]
    pub qualifying_goal_ids: Vec<String>,
    pub winner: ExpectedWinner,
    #[serde(default)]
    pub loser_ids: Vec<String>,
    #[serde(default)]
    pub below_threshold_goal_ids: Vec<String>,
    pub world_consultation_expected: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpectedWinner {
    None,
    GoalId { goal_id: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhraseObservation {
    pub inspection_captured: bool,
    pub arbitration_winner: Option<String>,
    pub differences: Vec<String>,
}

pub fn compare_phrase_expectation(
    expected: &PhraseExpected,
    inspection: Option<&VolitionInspectionCapture>,
    world: Option<&WorldPerceptionCapture>,
) -> PhraseObservation {
    let Some(capture) = inspection else {
        return PhraseObservation {
            differences: vec!["volition inspection capture missing".to_string()],
            ..PhraseObservation::default()
        };
    };
    let mut differences = Vec::new();
    let decision = capture.decision.as_ref();
    if decision.is_none() {
        differences.push("volition turn decision missing".to_string());
    }
    let winner = decision
        .and_then(|decision| decision.winner.as_ref())
        .map(|winner| winner.winner_goal_id.clone());
    let expected_winner = match &expected.winner {
        ExpectedWinner::None => None,
        ExpectedWinner::GoalId { goal_id } => Some(goal_id.as_str()),
    };
    if winner.as_deref() != expected_winner {
        differences.push(format!(
            "winner expected {:?}, observed {:?}",
            expected_winner,
            winner.as_deref()
        ));
    }
    if let Some(decision) = decision {
        let qualifying = decision
            .mode_bias_outcomes
            .iter()
            .map(|outcome| outcome.goal_id.clone())
            .collect::<Vec<_>>();
        if qualifying != expected.qualifying_goal_ids {
            differences.push(format!(
                "qualifying goals expected {:?}, observed {:?}",
                expected.qualifying_goal_ids, qualifying
            ));
        }
        let losers = qualifying
            .iter()
            .filter(|goal_id| Some(goal_id.as_str()) != winner.as_deref())
            .cloned()
            .collect::<Vec<_>>();
        if losers != expected.loser_ids {
            differences.push(format!(
                "losers expected {:?}, observed {:?}",
                expected.loser_ids, losers
            ));
        }
        let below_threshold = decision
            .below_threshold
            .iter()
            .map(|candidate| candidate.goal_id.clone())
            .collect::<Vec<_>>();
        if below_threshold != expected.below_threshold_goal_ids {
            differences.push(format!(
                "below-threshold goals expected {:?}, observed {:?}",
                expected.below_threshold_goal_ids, below_threshold
            ));
        }
    }
    let world_consulted = world.is_some_and(|capture| capture.consultation.is_some());
    if world_consulted != expected.world_consultation_expected {
        differences.push(format!(
            "world consultation expected {}, observed {}",
            expected.world_consultation_expected, world_consulted
        ));
    }
    PhraseObservation {
        inspection_captured: true,
        arbitration_winner: winner,
        differences,
    }
}

impl PhraseSet {
    pub fn content_hash(&self) -> anyhow::Result<String> {
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(self)?);
        Ok(hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

pub fn fixture_root() -> anyhow::Result<PathBuf> {
    let current = std::env::current_dir()?;
    for root in current.ancestors() {
        let candidate = root.join(FIXTURE_ROOT);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(current.join(FIXTURE_ROOT))
}

pub fn resolve_phrase_set(reference: &str) -> anyhow::Result<PathBuf> {
    let provided = PathBuf::from(reference);
    if provided.components().count() > 1 || provided.extension().is_some() {
        return Ok(provided);
    }
    let root = fixture_root()?;
    let candidate = root.join(format!("{reference}.phrases.json"));
    if candidate.exists() {
        return Ok(candidate);
    }
    let mut names = bundled_names(&root)?;
    names.sort();
    anyhow::bail!(
        "phrase set {reference} does not resolve to {}; bundled names: {}",
        candidate.display(),
        names.join(", ")
    );
}

pub fn load_phrase_set(reference: &str) -> anyhow::Result<PhraseSet> {
    let path = resolve_phrase_set(reference)?;
    let bytes = fs::read(&path).map_err(|error| {
        anyhow::anyhow!("failed to read phrase set {}: {error}", path.display())
    })?;
    let document = serde_json::from_slice(&bytes)
        .map_err(|error| anyhow::anyhow!("malformed phrase set {}: {error}", path.display()))?;
    validate_phrase_set(&document)?;
    Ok(document)
}

pub fn validate_phrase_set(set: &PhraseSet) -> anyhow::Result<()> {
    if set.id.trim().is_empty() || set.description.trim().is_empty() || set.phrases.is_empty() {
        anyhow::bail!("phrase set requires a non-empty id, description, and phrases");
    }
    for (index, phrase) in set.phrases.iter().enumerate() {
        if phrase.text.trim().is_empty() {
            anyhow::bail!("phrase {index} has empty text");
        }
        if matches!(
            &phrase.expected.winner,
            ExpectedWinner::GoalId { goal_id } if goal_id.trim().is_empty()
        ) {
            anyhow::bail!("phrase {index} expected winner has an empty goal_id");
        }
    }
    Ok(())
}

fn bundled_names(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut names = Vec::new();
    if !root.exists() {
        return Ok(names);
    }
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(name) = name.strip_suffix(".phrases.json") {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn bundled_smoke_resolves_and_loads() {
        let set = load_phrase_set("smoke").expect("bundled smoke");
        assert_eq!(set.phrases.len(), 2);
    }
    #[test]
    fn path_form_resolves_and_loads() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("custom.json");
        fs::write(
            &path,
            r#"{"id":"x","description":"x","phrases":[{"text":"x","expected":{"winner":{"kind":"none"},"world_consultation_expected":false}}]}"#,
        )
        .expect("write fixture");

        let set = load_phrase_set(path.to_str().expect("utf-8 path")).expect("path load");

        assert_eq!(set.id, "x");
    }

    #[test]
    fn unknown_name_lists_bundled_names_and_resolved_path() {
        let error = resolve_phrase_set("definitely-unknown")
            .expect_err("unknown name")
            .to_string();

        assert!(error.contains("definitely-unknown.phrases.json"));
        assert!(error.contains("bundled names: smoke"));
    }

    #[test]
    fn malformed_document_names_the_path_and_parse_error() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("malformed.json");
        fs::write(&path, b"{").expect("write malformed fixture");

        let error = load_phrase_set(path.to_str().expect("utf-8 path"))
            .expect_err("malformed document")
            .to_string();

        assert!(error.contains("malformed phrase set"));
        assert!(error.contains("malformed.json"));
    }

    #[test]
    fn missing_winner_is_a_malformed_document_error() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("missing-winner.json");
        fs::write(
            &path,
            r#"{"id":"x","description":"x","phrases":[{"text":"x","expected":{"world_consultation_expected":false}}]}"#,
        )
        .expect("write fixture");

        let error = load_phrase_set(path.to_str().expect("utf-8 path"))
            .expect_err("missing winner")
            .to_string();

        assert!(error.contains("malformed phrase set"));
        assert!(error.contains("missing field `winner`"));
    }
}
