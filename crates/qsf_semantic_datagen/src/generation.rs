use qsf_semantic_eval::{FrozenGoalRef, SliceTag};
use serde::Deserialize;

use crate::{GenerationOutput, INTERCHANGE_VERSION};

pub const GENERATION_PROMPT_VERSION: &str = "goalrel-gen-v1";
pub const MAX_HARD_NEGATIVE_SHARE_OF_BASE: f64 = 0.25;

/// The only goal material that can cross the generation prompt boundary.
/// Activation keywords deliberately have no representation in this type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalDescription {
    pub title: String,
    pub summary: String,
    pub tension_summaries: Vec<String>,
}

impl From<&FrozenGoalRef> for GoalDescription {
    fn from(goal: &FrozenGoalRef) -> Self {
        Self {
            title: goal.title.clone(),
            summary: goal.summary.clone(),
            tension_summaries: goal.tension_summaries.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenerationMode {
    Natural,
    ParaphraseCluster { cluster_id: String },
    ExplicitNegation,
    ImplicitNegation,
    QuotedSpeech,
    Hypothetical,
    SubjectConfusion,
    PunctuationCasingLoss,
    SyntheticAsr,
    RareHighCost,
    HardParaphrase { cluster_id: String },
    VagueNoneOfRoster,
}

impl GenerationMode {
    pub fn intended_slice_tags(&self) -> Vec<SliceTag> {
        match self {
            Self::Natural | Self::VagueNoneOfRoster => Vec::new(),
            Self::ParaphraseCluster { cluster_id } => {
                vec![SliceTag::ParaphraseCluster {
                    id: cluster_id.clone(),
                }]
            }
            Self::ExplicitNegation => vec![SliceTag::ExplicitNegation],
            Self::ImplicitNegation => vec![SliceTag::ImplicitNegation],
            Self::QuotedSpeech => vec![SliceTag::QuotedSpeech],
            Self::Hypothetical => vec![SliceTag::Hypothetical],
            Self::SubjectConfusion => vec![SliceTag::SubjectConfusion],
            Self::PunctuationCasingLoss => vec![SliceTag::PunctuationCasingLoss],
            Self::SyntheticAsr => vec![SliceTag::PunctuationCasingLoss, SliceTag::SyntheticAsr],
            Self::RareHighCost => vec![SliceTag::RareHighCost],
            Self::HardParaphrase { cluster_id } => vec![
                SliceTag::HardNegative,
                SliceTag::ParaphraseCluster {
                    id: cluster_id.clone(),
                },
            ],
        }
    }

    fn instruction(&self) -> &'static str {
        match self {
            Self::Natural => "Write natural conversational utterances about this goal description.",
            Self::ParaphraseCluster { .. } => {
                "Write close natural paraphrases of one shared meaning."
            }
            Self::ExplicitNegation => {
                "Write utterances with an explicit negation that still bears on this description."
            }
            Self::ImplicitNegation => {
                "Write utterances that imply a contrary stance without using an explicit negation."
            }
            Self::QuotedSpeech => {
                "Write utterances that quote another speaker while bearing on this description."
            }
            Self::Hypothetical => {
                "Write hypothetical or counterfactual utterances about this description."
            }
            Self::SubjectConfusion => {
                "Write utterances where whose goal or perspective is at issue needs careful reading."
            }
            Self::PunctuationCasingLoss => {
                "Write ordinary utterances suitable for later punctuation and casing loss."
            }
            Self::SyntheticAsr => {
                "Write ordinary utterances suitable for observed ASR-style casing, punctuation, and entity corruption."
            }
            Self::RareHighCost => {
                "Write rare, high-cost mistakes or stakes relevant to this description."
            }
            Self::HardParaphrase { .. } => {
                "Write deliberately difficult paraphrases that remain semantically close but avoid obvious wording."
            }
            Self::VagueNoneOfRoster => {
                "Write vague everyday utterances unrelated to any supplied goal. No goal is supplied for this request."
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptRequest {
    pub description: Option<GoalDescription>,
    pub mode: GenerationMode,
    pub count: usize,
}

pub fn build_prompt(request: &PromptRequest) -> Result<String, String> {
    if request.count == 0 {
        return Err("generation request count must be positive".to_string());
    }
    let description = match (&request.mode, &request.description) {
        (GenerationMode::VagueNoneOfRoster, None) => String::new(),
        (GenerationMode::VagueNoneOfRoster, Some(_)) => {
            return Err(
                "vague none_of_roster generation must not carry a goal description".to_string(),
            );
        }
        (_, Some(description)) => format!(
            "Goal title: {}\nGoal summary: {}\nTension summaries:\n{}\n",
            description.title,
            description.summary,
            description
                .tension_summaries
                .iter()
                .map(|summary| format!("- {summary}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        (_, None) => return Err("goal-conditioned generation requires a description".to_string()),
    };
    Ok(format!(
        "Generate {} English user utterances. {}\n{}Return JSON only: {{\"utterances\":[\"...\"]}}. Do not include labels, metadata, or explanations.",
        request.count,
        request.mode.instruction(),
        description
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationResponseContext {
    pub utterance_id_prefix: String,
    pub language: String,
    pub conditioning_goal_ref: Option<String>,
    pub mode: GenerationMode,
    pub session_id: String,
    pub semantic_cluster_id: String,
    pub generation_run_id: String,
    pub generator_model_id: String,
    pub synthetic_asr_seed: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelGenerationResponse {
    utterances: Vec<String>,
}

/// Parses only model text and combines it with pipeline-owned metadata. The model cannot set ids,
/// slices, conditioning, or the keyword-exposure flag.
pub fn parse_generation_response(
    response: &str,
    context: &GenerationResponseContext,
) -> Result<Vec<GenerationOutput>, String> {
    let response: ModelGenerationResponse = serde_json::from_str(response)
        .map_err(|error| format!("invalid generation model response: {error}"))?;
    if response.utterances.is_empty() {
        return Err("generation model response contains no utterances".to_string());
    }
    if context.mode == GenerationMode::VagueNoneOfRoster && context.conditioning_goal_ref.is_some()
    {
        return Err("vague none_of_roster output must have null conditioning_goal_ref".to_string());
    }
    if context.mode != GenerationMode::VagueNoneOfRoster && context.conditioning_goal_ref.is_none()
    {
        return Err("goal-conditioned output requires conditioning_goal_ref".to_string());
    }
    Ok(response
        .utterances
        .into_iter()
        .enumerate()
        .map(|(index, utterance)| GenerationOutput {
            interchange_version: INTERCHANGE_VERSION,
            utterance_id: format!("{}-{:02}", context.utterance_id_prefix, index + 1),
            utterance: match context.mode {
                GenerationMode::PunctuationCasingLoss => punctuation_casing_loss(&utterance),
                GenerationMode::SyntheticAsr => synthetic_asr_corrupt(
                    &utterance,
                    context.synthetic_asr_seed.wrapping_add(index as u64),
                ),
                _ => utterance,
            },
            language: context.language.clone(),
            conditioning_goal_ref: context.conditioning_goal_ref.clone(),
            intended_slice_tags: context.mode.intended_slice_tags(),
            session_id: context.session_id.clone(),
            semantic_cluster_id: context.semantic_cluster_id.clone(),
            generation_run_id: context.generation_run_id.clone(),
            generator_model_id: context.generator_model_id.clone(),
            prompt_version: GENERATION_PROMPT_VERSION.to_string(),
            saw_activation_keywords: false,
        })
        .collect())
}

/// Observed ASR-style corruption: loss of casing/punctuation plus deterministic entity mangling.
/// It intentionally does not produce random character-level typos.
pub fn synthetic_asr_corrupt(input: &str, seed: u64) -> String {
    let words: Vec<_> = input.split_whitespace().collect();
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let stripped = strip_ascii_punctuation(word);
            if is_entity_token(&words, index) {
                mangle_entity(&stripped, seed.wrapping_add(index as u64))
            } else {
                stripped.to_lowercase()
            }
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Observed transcript loss shared by punctuation/casing and synthetic-ASR variants.
pub fn punctuation_casing_loss(input: &str) -> String {
    input
        .chars()
        .filter(|character| !character.is_ascii_punctuation())
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn hard_negative_count(records: &[GenerationOutput]) -> usize {
    records
        .iter()
        .filter(|record| record.intended_slice_tags.contains(&SliceTag::HardNegative))
        .count()
}

pub fn hard_negative_within_distribution(records: &[GenerationOutput]) -> bool {
    let hard = hard_negative_count(records);
    let base = records.len().saturating_sub(hard);
    hard == 0 || (base > 0 && (hard as f64 / base as f64) <= MAX_HARD_NEGATIVE_SHARE_OF_BASE)
}

fn strip_ascii_punctuation(word: &str) -> String {
    word.chars()
        .filter(|character| !character.is_ascii_punctuation())
        .collect()
}

fn is_entity_token(words: &[&str], index: usize) -> bool {
    let word = strip_ascii_punctuation(words[index]);
    let mut alphabetic = word.chars().filter(|character| character.is_alphabetic());
    let Some(first) = alphabetic.next() else {
        return false;
    };
    let has_internal_uppercase = alphabetic.any(|character| character.is_uppercase());
    if has_internal_uppercase {
        return true;
    }
    let title_cased = first.is_uppercase();
    if !title_cased {
        return false;
    }
    let neighboring_title_case = |neighbor: &str| {
        strip_ascii_punctuation(neighbor)
            .chars()
            .find(|character| character.is_alphabetic())
            .is_some_and(char::is_uppercase)
    };
    let follows_sentence_boundary = index == 0
        || words[index - 1]
            .chars()
            .last()
            .is_some_and(|character| matches!(character, '.' | '!' | '?'));
    words
        .get(index + 1)
        .is_some_and(|neighbor| neighboring_title_case(neighbor))
        || !follows_sentence_boundary
}

fn mangle_entity(word: &str, seed: u64) -> String {
    let lowercase: Vec<char> = word.chars().flat_map(char::to_lowercase).collect();
    if lowercase.len() < 2 {
        return lowercase.into_iter().collect();
    }
    let hash = lowercase
        .iter()
        .fold(seed ^ 0x517c_c1b7_2722_0a95, |hash, character| {
            hash.rotate_left(7) ^ u64::from(u32::from(*character))
        });
    let split_at = 1 + hash as usize % (lowercase.len() - 1);
    format!(
        "{} {}",
        lowercase[..split_at].iter().collect::<String>(),
        lowercase[split_at..].iter().collect::<String>()
    )
}
