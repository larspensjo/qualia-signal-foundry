use qsf_semantic_eval::{FrozenGoalRef, RosterSnapshot, SliceTag};
use serde::Deserialize;

use crate::{CompletionRequest, GenerationOutput, INTERCHANGE_VERSION, ModelTransport, TokenUsage};

pub const GENERATION_PROMPT_VERSION: &str = "goalrel-gen-v2";
pub const GENERATOR_MODEL_ID: &str = "gpt-5.4-nano";
pub const MAX_HARD_NEGATIVE_SHARE_OF_BASE: f64 = 0.25;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationRun {
    pub records: Vec<GenerationOutput>,
    pub usage: Option<TokenUsage>,
}

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
            "The utterances must bear on the following goal. The goal belongs to the assistant, \
not the user — do not restate it or act it out; write what a user whose words touch on this \
goal's subject matter would say.\nGoal title: {}\nGoal summary: {}\nTension summaries:\n{}\n",
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
        "Generate {} English utterances spoken by a human user to their AI assistant. \
Write only the user's side of the conversation, in the user's own voice — things the user \
would say about their own life, work, thoughts, or questions. Never write the assistant's \
replies or an assistant-like voice (no offering help, no inviting the user to share). {}\n\
{}Return JSON only: {{\"utterances\":[\"...\"]}}. Do not include labels, metadata, or explanations.",
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

/// Runs the deterministic generation request schedule through the supplied transport.
/// Prompt construction and response parsing remain transport-independent and testable.
pub fn run_generation<T: ModelTransport>(
    transport: &T,
    roster: &RosterSnapshot,
    generation_run_id: &str,
) -> Result<GenerationRun, String> {
    let goal = roster
        .goals
        .first()
        .ok_or_else(|| "roster has no goals".to_string())?;
    let mut records = Vec::new();
    let mut usage = TokenUsage {
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
    };
    let mut has_usage = false;
    for partition in ["validation", "test"] {
        for (index, mode) in generation_modes(partition).into_iter().enumerate() {
            let description = if mode == GenerationMode::VagueNoneOfRoster {
                None
            } else {
                Some(GoalDescription::from(goal))
            };
            let prompt = build_prompt(&PromptRequest {
                description,
                mode: mode.clone(),
                count: 8,
            })?;
            let response = transport.complete(&CompletionRequest {
                model_id: GENERATOR_MODEL_ID.to_string(),
                prompt,
                goal_ref: goal.goal_ref.clone(),
                run_id: generation_run_id.to_string(),
                utterance_id: None,
            })?;
            if let Some(call_usage) = response.usage {
                usage.add(call_usage);
                has_usage = true;
            }
            records.extend(parse_generation_response(
                &response.content,
                &GenerationResponseContext {
                    utterance_id_prefix: format!("{generation_run_id}-{partition}-{index}"),
                    language: "en".to_string(),
                    conditioning_goal_ref: if mode == GenerationMode::VagueNoneOfRoster {
                        None
                    } else {
                        Some(goal.goal_ref.clone())
                    },
                    mode,
                    session_id: format!("{generation_run_id}-session-{partition}"),
                    semantic_cluster_id: format!("{generation_run_id}-semantic-{partition}"),
                    generation_run_id: generation_run_id.to_string(),
                    generator_model_id: GENERATOR_MODEL_ID.to_string(),
                    synthetic_asr_seed: 20260721,
                },
            )?);
        }
    }
    Ok(GenerationRun {
        records,
        usage: has_usage.then_some(usage),
    })
}

fn generation_modes(partition: &str) -> Vec<GenerationMode> {
    let mut modes = vec![GenerationMode::Natural];
    for cluster in 1..=4 {
        modes.push(GenerationMode::ParaphraseCluster {
            cluster_id: format!("{partition}-cluster-{cluster}"),
        });
    }
    modes.extend([
        GenerationMode::ExplicitNegation,
        GenerationMode::ImplicitNegation,
        GenerationMode::QuotedSpeech,
        GenerationMode::Hypothetical,
        GenerationMode::SubjectConfusion,
        GenerationMode::PunctuationCasingLoss,
        GenerationMode::SyntheticAsr,
        GenerationMode::RareHighCost,
        GenerationMode::HardParaphrase {
            cluster_id: format!("{partition}-hard-cluster"),
        },
        GenerationMode::VagueNoneOfRoster,
    ]);
    modes
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

/// Human-readable rendering of a generated pool for operator review. Shared
/// run metadata is stated once; each utterance is listed by number with its
/// slice tags only when it carries any.
pub fn render_generation_report(
    goal_title: Option<&str>,
    records: &[GenerationOutput],
) -> Result<String, String> {
    let first = records
        .first()
        .ok_or_else(|| "cannot render a report for an empty generation pool".to_string())?;
    let mut report = format!(
        "generated {} utterance(s) — model {}, run {}\n",
        records.len(),
        first.generator_model_id,
        first.generation_run_id
    );
    report.push_str(&format!(
        "goal: {}\n",
        goal_title.unwrap_or("(none — goal-unconditioned batch)")
    ));
    report.push_str(&format!(
        "prompt {}, language {}, session {}, cluster {}, saw_activation_keywords={}\n\n",
        first.prompt_version,
        first.language,
        first.session_id,
        first.semantic_cluster_id,
        first.saw_activation_keywords
    ));
    for (index, record) in records.iter().enumerate() {
        report.push_str(&format!("{:3}. {}\n", index + 1, record.utterance));
        if !record.intended_slice_tags.is_empty() {
            let tags = record
                .intended_slice_tags
                .iter()
                .map(slice_tag_display)
                .collect::<Vec<_>>()
                .join(", ");
            report.push_str(&format!("     slices: {tags}\n"));
        }
    }
    Ok(report)
}

/// Renders a slice tag with the same snake_case vocabulary the interchange
/// artifacts use, keeping serde as the one source of truth for tag names.
fn slice_tag_display(tag: &SliceTag) -> String {
    let value = serde_json::to_value(tag).unwrap_or_default();
    let kind = value
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    match value.get("id").and_then(serde_json::Value::as_str) {
        Some(id) => format!("{kind}:{id}"),
        None => kind,
    }
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
