use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::realtime::volition_inspection_capture::VolitionInspectionCapture;

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
    pub explicit_topic_expected: bool,
    #[serde(default)]
    pub world_consultation_expected: Option<ExpectedWorldConsultation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExpectedWorldConsultation {
    pub trigger: qsf_diagnostics::WorldConsultationTrigger,
    pub required_anchors: Vec<String>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhraseExpectationDifferences {
    pub phrase_index: usize,
    pub differences: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObservedWorldConsultation {
    exchange_index: usize,
    trigger: qsf_diagnostics::WorldConsultationTrigger,
    required_anchors: Vec<String>,
}

pub fn compare_phrase_expectation(
    expected: &PhraseExpected,
    inspection: Option<&VolitionInspectionCapture>,
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
    PhraseObservation {
        inspection_captured: true,
        arbitration_winner: winner,
        differences,
    }
}

/// Compare declared consultations with authoritative diagnostics records as a run-level set.
/// A deferred lookup is recorded on the next exchange, so its record exchange cannot identify
/// the phrase that requested it; trigger and required anchors remain stable across deferral.
pub fn compare_world_consultation_expectations(
    phrases: &PhraseSet,
    diagnostics: &[u8],
) -> Vec<PhraseExpectationDifferences> {
    let observed = parse_world_consultations(diagnostics);
    let mut used = vec![false; observed.len()];
    let mut differences = Vec::new();
    for (phrase_index, expected) in
        phrases
            .phrases
            .iter()
            .enumerate()
            .filter_map(|(index, phrase)| {
                phrase
                    .expected
                    .world_consultation_expected
                    .as_ref()
                    .map(|expected| (index, expected))
            })
    {
        if let Some(position) = observed.iter().enumerate().position(|(position, actual)| {
            !used[position]
                && actual.trigger == expected.trigger
                && actual.required_anchors == expected.required_anchors
        }) {
            used[position] = true;
        } else {
            differences.push(PhraseExpectationDifferences {
                phrase_index,
                differences: vec![format!(
                    "world consultation expected trigger {:?} with anchors {:?}, observed no matching diagnostics record",
                    expected.trigger, expected.required_anchors
                )],
            });
        }
    }
    for actual in observed
        .iter()
        .enumerate()
        .filter_map(|(position, actual)| (!used[position]).then_some(actual))
    {
        differences.push(PhraseExpectationDifferences {
            phrase_index: actual
                .exchange_index
                .min(phrases.phrases.len().saturating_sub(1)),
            differences: vec![format!(
                "unexpected world consultation trigger {:?} with anchors {:?} recorded at exchange {}",
                actual.trigger, actual.required_anchors, actual.exchange_index
            )],
        });
    }
    differences
}

fn parse_world_consultations(diagnostics: &[u8]) -> Vec<ObservedWorldConsultation> {
    String::from_utf8_lossy(diagnostics)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| {
            record.get("kind").and_then(serde_json::Value::as_str)
                == Some("world_consultation_performed")
        })
        .filter_map(|record| {
            let exchange_index = record.get("exchange_index")?.as_u64()? as usize;
            let trigger = serde_json::from_value(record.pointer("/trace/trigger")?.clone()).ok()?;
            let required_anchors =
                serde_json::from_value(record.pointer("/trace/required_anchors")?.clone()).ok()?;
            Some(ObservedWorldConsultation {
                exchange_index,
                trigger,
                required_anchors,
            })
        })
        .collect()
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
        let designed = load_phrase_set("designed").expect("bundled designed");
        assert_eq!(designed.phrases.len(), 12);
    }
    #[test]
    fn path_form_resolves_and_loads() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("custom.json");
        fs::write(
            &path,
            r#"{"id":"x","description":"x","phrases":[{"text":"x","expected":{"winner":{"kind":"none"},"explicit_topic_expected":false}}]}"#,
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
        assert!(error.contains("bundled names: designed, smoke"));
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
            r#"{"id":"x","description":"x","phrases":[{"text":"x","expected":{"explicit_topic_expected":false}}]}"#,
        )
        .expect("write fixture");

        let error = load_phrase_set(path.to_str().expect("utf-8 path"))
            .expect_err("missing winner")
            .to_string();

        assert!(error.contains("malformed phrase set"));
        assert!(error.contains("missing field `winner`"));
    }

    #[test]
    fn bundled_phrase_sets_match_the_warm_start_selection_contract() {
        use qsf_volition::{
            InitiativeOutput, arbitrate_with_mode, execute_initiative,
            explicit_topic_world_consultation_request, realtime_seed_fixture, select_goals_ranked,
        };

        let fixture = realtime_seed_fixture();
        let warm_state = crate::scripted_conversation::load_seed_templates()
            .expect("seed templates")
            .volition
            .state;
        for set_name in ["smoke", "designed"] {
            let set = load_phrase_set(set_name).expect("bundled phrase set");
            for (index, phrase) in set.phrases.iter().enumerate() {
                // Reloading the warm state verifies each phrase's declared starting-state
                // contract without encoding live side effects into this offline fixture gate.
                let state = warm_state.clone();
                let ranked = select_goals_ranked(&phrase.text, &state, &fixture);
                let outcome = arbitrate_with_mode(ranked.selected, &fixture, state.mode)
                    .expect("matched selection or below-threshold candidate");
                let (winner, losers) =
                    outcome
                        .qualified
                        .as_ref()
                        .map_or((None, Vec::new()), |result| {
                            (
                                Some(result.winner.goal.id.clone()),
                                result
                                    .losers
                                    .iter()
                                    .map(|loser| loser.selection.goal.id.clone())
                                    .collect(),
                            )
                        });
                let qualifying = outcome.qualified.as_ref().map_or_else(Vec::new, |result| {
                    std::iter::once(result.winner.goal.id.clone())
                        .chain(
                            result
                                .losers
                                .iter()
                                .map(|loser| loser.selection.goal.id.clone()),
                        )
                        .collect()
                });
                let expected_winner = match &phrase.expected.winner {
                    ExpectedWinner::None => None,
                    ExpectedWinner::GoalId { goal_id } => Some(goal_id.clone()),
                };
                assert_eq!(
                    qualifying,
                    phrase.expected.qualifying_goal_ids,
                    "{set_name} turn {} qualifying",
                    index + 1
                );
                assert_eq!(
                    winner,
                    expected_winner,
                    "{set_name} turn {} winner",
                    index + 1
                );
                assert_eq!(
                    losers,
                    phrase.expected.loser_ids,
                    "{set_name} turn {} losers",
                    index + 1
                );
                assert_eq!(
                    outcome
                        .below_threshold
                        .iter()
                        .map(|candidate| candidate.selection.goal.id.clone())
                        .collect::<Vec<_>>(),
                    phrase.expected.below_threshold_goal_ids,
                    "{set_name} turn {} below threshold",
                    index + 1
                );
                assert_eq!(
                    explicit_topic_world_consultation_request(&phrase.text).is_some(),
                    phrase.expected.explicit_topic_expected,
                    "{set_name} turn {} explicit topic",
                    index + 1
                );
                let goal_consultation = outcome.qualified.as_ref().and_then(|result| {
                    match execute_initiative(&result.winner.initiative, &result.winner.goal) {
                        InitiativeOutput::WorldConsultationRequested { query_terms } => {
                            Some(ExpectedWorldConsultation {
                                trigger: qsf_diagnostics::WorldConsultationTrigger::GoalActivation,
                                required_anchors: query_terms
                                    .into_iter()
                                    .map(|term| term.term)
                                    .collect(),
                            })
                        }
                        _ => None,
                    }
                });
                let explicit_consultation = explicit_topic_world_consultation_request(&phrase.text)
                    .map(|request| ExpectedWorldConsultation {
                        trigger: qsf_diagnostics::WorldConsultationTrigger::ExplicitCurrentTopic,
                        required_anchors: request.required_anchors,
                    });
                assert_eq!(
                    goal_consultation.or(explicit_consultation),
                    phrase.expected.world_consultation_expected,
                    "{set_name} turn {} world consultation effect",
                    index + 1
                );
            }
        }
        let designed = load_phrase_set("designed").expect("designed");
        let upper = &designed.phrases[7].text;
        let lower = &designed.phrases[8].text;
        assert_eq!(upper.replace("Grok", "grok"), *lower);

        let cold_state = qsf_volition::VolitionState::from_fixture(&fixture);
        for phrase in load_phrase_set("smoke").expect("smoke").phrases {
            let ids = |state: &qsf_volition::VolitionState| {
                let outcome = arbitrate_with_mode(
                    select_goals_ranked(&phrase.text, state, &fixture).selected,
                    &fixture,
                    state.mode,
                )
                .expect("smoke selection");
                (
                    outcome
                        .qualified
                        .as_ref()
                        .map(|result| result.winner.goal.id.clone()),
                    outcome
                        .below_threshold
                        .iter()
                        .map(|candidate| candidate.selection.goal.id.clone())
                        .collect::<Vec<_>>(),
                )
            };
            assert_eq!(
                ids(&warm_state),
                ids(&cold_state),
                "smoke fixture is selector-equivalent under cold start"
            );
        }
    }

    #[test]
    fn consultation_comparison_uses_diagnostic_trigger_and_anchors_across_deferral() {
        let set: PhraseSet = serde_json::from_value(serde_json::json!({
            "id": "x",
            "description": "x",
            "phrases": [
                {
                    "text": "capitalized topic",
                    "expected": {
                        "winner": {"kind": "none"},
                        "explicit_topic_expected": true,
                        "world_consultation_expected": {
                            "trigger": "explicit_current_topic",
                            "required_anchors": ["grok"]
                        }
                    }
                },
                {
                    "text": "lowercase control",
                    "expected": {
                        "winner": {"kind": "none"},
                        "explicit_topic_expected": false
                    }
                }
            ]
        }))
        .expect("phrase set");
        let diagnostics = br#"{"kind":"world_consultation_performed","exchange_index":1,"trace":{"trigger":"explicit_current_topic","required_anchors":["grok"]}}"#;

        assert!(compare_world_consultation_expectations(&set, diagnostics).is_empty());
    }

    #[test]
    fn consultation_comparison_reports_unexpected_goal_activation_without_shifting_expected_match()
    {
        let set = load_phrase_set("designed").expect("designed");
        let diagnostics = concat!(
            "{\"kind\":\"world_consultation_performed\",\"exchange_index\":1,\"trace\":{\"trigger\":\"goal_activation\",\"required_anchors\":[\"ai\"]}}\n",
            "{\"kind\":\"world_consultation_performed\",\"exchange_index\":8,\"trace\":{\"trigger\":\"explicit_current_topic\",\"required_anchors\":[\"grok\"]}}\n",
            "{\"kind\":\"world_consultation_performed\",\"exchange_index\":11,\"trace\":{\"trigger\":\"goal_activation\",\"required_anchors\":[\"world\",\"society\"]}}"
        );

        let differences = compare_world_consultation_expectations(&set, diagnostics.as_bytes());

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].phrase_index, 1);
        assert!(differences[0].differences[0].contains("unexpected world consultation"));
    }
}
