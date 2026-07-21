use qsf_semantic_eval::{FrozenGoalRef, RosterSnapshot, SliceTag};
use serde::Deserialize;

use crate::{CompletionRequest, GenerationOutput, INTERCHANGE_VERSION, ModelTransport, TokenUsage};

pub const GENERATION_PROMPT_VERSION: &str = "goalrel-gen-v3";
pub const GENERATOR_MODEL_ID: &str = "gpt-5.4-nano";
pub const MAX_HARD_NEGATIVE_SHARE_OF_BASE: f64 = 0.25;
/// Live models are stochastic against the mode validators, so a rejected batch is
/// re-requested up to this many times before the run fails loudly.
pub const MAX_GENERATION_BATCH_ATTEMPTS: usize = 3;

const EXPLICIT_NEGATOR_WORDS: &[&str] = &[
    "no",
    "not",
    "never",
    "cannot",
    "nothing",
    "nobody",
    "none",
    "neither",
    "nor",
    "don't",
    "isn't",
    "can't",
    "won't",
    "doesn't",
    "didn't",
    "aren't",
    "weren't",
    "wasn't",
    "couldn't",
    "shouldn't",
    "wouldn't",
    "haven't",
    "hasn't",
    "hadn't",
    "mustn't",
    "needn't",
    "shan't",
    "ain't",
];
const HYPOTHETICAL_FRAMING_MARKERS: &[&str] =
    &["what if", "suppose", "imagine", "if i ever", "if i were"];
const HARD_NEGATIVE_FORBIDDEN_WORDS: &[&str] = &[
    "pry",
    "prying",
    "pried",
    "private",
    "privacy",
    "personal",
    "personally",
    "boundary",
    "boundaries",
    "gossip",
    "gossiping",
    "gossiped",
    "probe",
    "probed",
    "probes",
    "probing",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationRun {
    pub records: Vec<GenerationOutput>,
    pub usage: Option<TokenUsage>,
    pub cluster_anchors: Vec<GenerationClusterAnchor>,
}

/// Operator-only evidence for the semantic proposition shared by a paraphrase batch.
/// This intentionally never enters the generation-output interchange artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationClusterAnchor {
    pub cluster_id: String,
    pub anchor: String,
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

    fn name(&self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::ParaphraseCluster { .. } => "paraphrase_cluster",
            Self::ExplicitNegation => "explicit_negation",
            Self::ImplicitNegation => "implicit_negation",
            Self::QuotedSpeech => "quoted_speech",
            Self::Hypothetical => "hypothetical",
            Self::SubjectConfusion => "subject_confusion",
            Self::PunctuationCasingLoss => "punctuation_casing_loss",
            Self::SyntheticAsr => "synthetic_asr",
            Self::RareHighCost => "rare_high_cost",
            Self::HardParaphrase { .. } => "hard_negative",
            Self::VagueNoneOfRoster => "none_of_roster",
        }
    }

    fn cluster_id(&self) -> Option<&str> {
        match self {
            Self::ParaphraseCluster { cluster_id } | Self::HardParaphrase { cluster_id } => {
                Some(cluster_id)
            }
            _ => None,
        }
    }

    fn instruction(&self) -> String {
        match self {
            Self::Natural => "Write natural conversational utterances about this goal description.".to_string(),
            Self::ParaphraseCluster { .. } => {
                "First fix ONE concrete anchor proposition with specific actors, event, stance, and consequence. Return that proposition in the `anchor` field. Then write every utterance as a wording-level paraphrase of exactly that proposition: preserve its actors, event, stance, and consequence in every line, varying only the wording."
                    .to_string()
            }
            Self::ExplicitNegation => {
                "Write utterances with an explicit negation that still bears on this description.".to_string()
            }
            Self::ImplicitNegation => format!(
                "Write utterances that imply a contrary or resisting stance using only positive phrasing: prefer an alternative, redirect the topic, or leave something alone (for example \"I'd rather keep the focus on her project\", \"I leave his weekend plans alone\"). Every grammatical negator and negative quantifier is forbidden: {}. Express the contrary stance entirely without them.",
                EXPLICIT_NEGATOR_WORDS.join(", ")
            ),
            Self::QuotedSpeech => {
                "Write utterances that quote another speaker while bearing on this description.".to_string()
            }
            Self::Hypothetical => format!(
                "Write an explicitly imagined event about this description using one of these framing forms in every utterance: {}. Reject memories, habits, and timeless `if someone ...` policies.",
                HYPOTHETICAL_FRAMING_MARKERS.join(", ")
            ),
            Self::SubjectConfusion => "Write utterances with at least three distinguishable roles (for example the user, assistant, and two other people). Make the relevant reluctance or pressure belong to someone other than the grammatical speaker, so whose perspective matters requires careful reading.".to_string(),
            Self::PunctuationCasingLoss => {
                "Write ordinary utterances suitable for later punctuation and casing loss.".to_string()
            }
            Self::SyntheticAsr => "Write ordinary utterances suitable for observed ASR-style casing, punctuation, and entity corruption. Every utterance must contain a personal name or a product/company name that can be plausibly mangled by ASR.".to_string(),
            Self::RareHighCost => "Write rare, high-stakes situations relevant to this description. Every utterance must include both a rare or high-stakes setting and a concrete severe consequence of crossing or protecting the boundary, such as loss of employment, safety, custody, legal exposure, medical confidentiality, or severe reputational harm. Naming divorce, finances, therapy, or an ordinary workplace exchange alone is insufficient.".to_string(),
            Self::HardParaphrase { .. } => format!(
                "First fix ONE concrete anchor proposition with specific actors, event, stance, and consequence. Return that proposition in the `anchor` field. Then write every utterance as a wording-level paraphrase of exactly that proposition, preserving its actors, event, stance, and consequence in every line while varying only the wording. Make the evidence indirect and behavioral rather than a summary of the goal. Do not use any of these words: {}.",
                HARD_NEGATIVE_FORBIDDEN_WORDS.join(", ")
            ),
            Self::VagueNoneOfRoster => "Write vague everyday utterances unrelated to any supplied goal. No goal is supplied for this request. Do not express reluctance or refusal to talk, because those can engage a boundaries-related goal even when vague.".to_string(),
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
    let response_shape = if request.mode.cluster_id().is_some() {
        "{\"anchor\":\"one concrete proposition\",\"utterances\":[\"...\"]}"
    } else {
        "{\"utterances\":[\"...\"]}"
    };
    Ok(format!(
        "Generate {} English utterances spoken by a human user to their AI assistant. \
The speaker is always a human user talking TO their AI assistant, typically about interactions \
with other humans. The user may tell the AI not to pry into other people's lives, but must never \
address the AI as though it were a human interlocutor with its own family, work, or personal life; \
never offer the AI privacy; and never adopt an assistant-like voice. Write only the user's side of \
the conversation, in the user's own voice — things the user would say about their own life, work, \
thoughts, or questions. Never write the assistant's replies or an assistant-like voice (no offering \
help, no inviting the user to share). Across the batch, vary actors, settings, speech acts, stances, \
tenses, and consequences; never reuse one sentence skeleton with synonym swaps. {}\n\
{}Return JSON only: {}. Do not include labels, metadata, or explanations.",
        request.count,
        request.mode.instruction(),
        description,
        response_shape
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
    anchor: Option<String>,
    utterances: Vec<String>,
}

struct ParsedGenerationResponse {
    records: Vec<GenerationOutput>,
    cluster_anchor: Option<String>,
}

/// Parses only model text and combines it with pipeline-owned metadata. The model cannot set ids,
/// slices, conditioning, or the keyword-exposure flag.
pub fn parse_generation_response(
    response: &str,
    context: &GenerationResponseContext,
) -> Result<Vec<GenerationOutput>, String> {
    Ok(parse_generation_response_with_anchor(response, context)?.records)
}

fn parse_generation_response_with_anchor(
    response: &str,
    context: &GenerationResponseContext,
) -> Result<ParsedGenerationResponse, String> {
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
    let cluster_anchor = response.anchor.filter(|anchor| !anchor.trim().is_empty());
    if context.mode.cluster_id().is_some() && cluster_anchor.is_none() {
        return Err(format!(
            "mode {} requires a non-empty anchor field",
            context.mode.name()
        ));
    }
    let records = response
        .utterances
        .into_iter()
        .enumerate()
        .map(|(index, utterance)| {
            let utterance_id = format!("{}-{:02}", context.utterance_id_prefix, index + 1);
            let utterance = match context.mode {
                GenerationMode::PunctuationCasingLoss => punctuation_casing_loss(&utterance),
                GenerationMode::SyntheticAsr => synthetic_asr_corrupt(
                    &utterance,
                    context.synthetic_asr_seed.wrapping_add(index as u64),
                )
                .map_err(|error| {
                    format!(
                        "mode {} validation failed for utterance id {utterance_id}: {error}",
                        context.mode.name()
                    )
                })?,
                _ => utterance,
            };
            Ok(GenerationOutput {
                interchange_version: INTERCHANGE_VERSION,
                utterance_id,
                utterance,
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
        })
        .collect::<Result<Vec<_>, String>>()?;
    validate_mode_outputs(&context.mode, &records)?;
    Ok(ParsedGenerationResponse {
        records,
        cluster_anchor,
    })
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
    let mut cluster_anchors = Vec::new();
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
            let context = GenerationResponseContext {
                utterance_id_prefix: format!("{generation_run_id}-{partition}-{index}"),
                language: "en".to_string(),
                conditioning_goal_ref: if mode == GenerationMode::VagueNoneOfRoster {
                    None
                } else {
                    Some(goal.goal_ref.clone())
                },
                mode: mode.clone(),
                session_id: format!("{generation_run_id}-session-{partition}"),
                semantic_cluster_id: format!("{generation_run_id}-semantic-{partition}"),
                generation_run_id: generation_run_id.to_string(),
                generator_model_id: GENERATOR_MODEL_ID.to_string(),
                synthetic_asr_seed: 20260721,
            };
            let mut attempt = 1usize;
            let parsed = loop {
                let response = transport.complete(&CompletionRequest {
                    model_id: GENERATOR_MODEL_ID.to_string(),
                    prompt: prompt.clone(),
                    goal_ref: goal.goal_ref.clone(),
                    run_id: generation_run_id.to_string(),
                    utterance_id: None,
                })?;
                if let Some(call_usage) = response.usage {
                    usage.add(call_usage);
                    has_usage = true;
                }
                match parse_generation_response_with_anchor(&response.content, &context) {
                    Ok(parsed) => break parsed,
                    Err(error) if attempt < MAX_GENERATION_BATCH_ATTEMPTS => {
                        engine_logging::engine_warn!(
                            "goal relevance generation batch rejected, re-requesting run_id={} mode={} attempt={}/{} error={}",
                            generation_run_id,
                            mode.name(),
                            attempt,
                            MAX_GENERATION_BATCH_ATTEMPTS,
                            error
                        );
                        attempt += 1;
                    }
                    Err(error) => {
                        engine_logging::engine_error!(
                            "goal relevance generation rejected run_id={} mode={} attempts={} error={}",
                            generation_run_id,
                            mode.name(),
                            attempt,
                            error
                        );
                        return Err(error);
                    }
                }
            };
            if let (Some(cluster_id), Some(anchor)) = (mode.cluster_id(), parsed.cluster_anchor) {
                cluster_anchors.push(GenerationClusterAnchor {
                    cluster_id: cluster_id.to_string(),
                    anchor,
                });
            }
            records.extend(parsed.records);
        }
    }
    Ok(GenerationRun {
        records,
        usage: has_usage.then_some(usage),
        cluster_anchors,
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
pub fn synthetic_asr_corrupt(input: &str, seed: u64) -> Result<String, String> {
    let tokens = transcript_tokens(input);
    let mut mangled_entity = false;
    let output = tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            if is_entity_token(&tokens, index) && token.text.chars().count() >= 2 {
                mangled_entity = true;
                mangle_entity(&token.text, seed.wrapping_add(index as u64))
            } else {
                token.text.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if mangled_entity {
        Ok(output)
    } else {
        Err(
            "synthetic ASR requires at least one mangle-able personal or product/company name"
                .to_string(),
        )
    }
}

/// Observed transcript loss shared by punctuation/casing and synthetic-ASR variants.
pub fn punctuation_casing_loss(input: &str) -> String {
    transcript_tokens(input)
        .into_iter()
        .map(|token| token.text.to_lowercase())
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

#[derive(Debug, Eq, PartialEq)]
struct TranscriptToken {
    text: String,
    follows_sentence_boundary: bool,
}

fn transcript_tokens(input: &str) -> Vec<TranscriptToken> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut token_follows_sentence_boundary = true;
    let mut next_token_follows_sentence_boundary = true;

    let finish_token =
        |tokens: &mut Vec<TranscriptToken>, text: &mut String, follows_sentence_boundary| {
            if !text.is_empty() {
                tokens.push(TranscriptToken {
                    text: std::mem::take(text),
                    follows_sentence_boundary,
                });
            }
        };

    for character in input.chars() {
        if character.is_alphanumeric() {
            if text.is_empty() {
                token_follows_sentence_boundary = next_token_follows_sentence_boundary;
                next_token_follows_sentence_boundary = false;
            }
            text.push(character);
        } else if matches!(character, '\'' | '\u{2018}' | '\u{2019}') {
            // Apostrophes disappear without splitting contractions or possessives.
        } else {
            finish_token(&mut tokens, &mut text, token_follows_sentence_boundary);
            if matches!(character, '.' | '!' | '?') {
                next_token_follows_sentence_boundary = true;
            }
        }
    }
    finish_token(&mut tokens, &mut text, token_follows_sentence_boundary);
    tokens
}

fn is_entity_token(tokens: &[TranscriptToken], index: usize) -> bool {
    let token = &tokens[index];
    let mut alphabetic = token
        .text
        .chars()
        .filter(|character| character.is_alphabetic());
    let Some(first) = alphabetic.next() else {
        return false;
    };
    let has_internal_uppercase = alphabetic.any(|character| character.is_uppercase());
    if has_internal_uppercase {
        return true;
    }
    first.is_uppercase() && !token.follows_sentence_boundary
}

fn validate_mode_outputs(
    mode: &GenerationMode,
    records: &[GenerationOutput],
) -> Result<(), String> {
    let invalid_ids = match mode {
        GenerationMode::ImplicitNegation => records
            .iter()
            .filter(|record| contains_explicit_negator(&record.utterance))
            .map(|record| record.utterance_id.clone())
            .collect(),
        GenerationMode::Hypothetical => records
            .iter()
            .filter(|record| !contains_hypothetical_marker(&record.utterance))
            .map(|record| record.utterance_id.clone())
            .collect(),
        GenerationMode::HardParaphrase { .. } => records
            .iter()
            .filter(|record| contains_hard_negative_forbidden_word(&record.utterance))
            .map(|record| record.utterance_id.clone())
            .collect(),
        _ => Vec::new(),
    };
    if invalid_ids.is_empty() {
        return Ok(());
    }
    Err(format!(
        "mode {} validation failed for utterance ids {}",
        mode.name(),
        invalid_ids.join(", ")
    ))
}

fn contains_explicit_negator(utterance: &str) -> bool {
    normalized_words(utterance).iter().any(|word| {
        let compact = word.replace('\'', "");
        EXPLICIT_NEGATOR_WORDS
            .iter()
            .any(|negator| negator.replace('\'', "") == compact)
    })
}

fn contains_hypothetical_marker(utterance: &str) -> bool {
    let words = normalized_words(utterance);
    HYPOTHETICAL_FRAMING_MARKERS.iter().any(|marker| {
        let marker_words = normalized_words(marker);
        words
            .windows(marker_words.len())
            .any(|window| window == marker_words)
    })
}

fn contains_hard_negative_forbidden_word(utterance: &str) -> bool {
    normalized_words(utterance)
        .iter()
        .any(|word| HARD_NEGATIVE_FORBIDDEN_WORDS.contains(&word.as_str()))
}

fn normalized_words(utterance: &str) -> Vec<String> {
    let mut normalized = String::new();
    for character in utterance.chars() {
        if matches!(character, '\u{2018}' | '\u{2019}') {
            normalized.push('\'');
        } else {
            normalized.extend(character.to_lowercase());
        }
    }
    normalized
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .map(ToOwned::to_owned)
        .collect()
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
