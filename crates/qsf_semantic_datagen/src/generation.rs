use std::collections::{BTreeMap, BTreeSet};

use qsf_semantic_eval::{FrozenGoalRef, RosterSnapshot, SliceTag};
use serde::{Deserialize, Serialize};

use crate::{CompletionRequest, GenerationOutput, INTERCHANGE_VERSION, ModelTransport, TokenUsage};

pub const GENERATION_PROMPT_VERSION: &str = "goalrel-gen-v6";
pub const GENERATOR_MODEL_ID: &str = "gpt-5.4-nano";
pub const SEMANTIC_GENERATOR_MODEL_ID: &str = "gpt-5.4-mini";
pub const MAX_HARD_NEGATIVE_SHARE_OF_BASE: f64 = 0.25;
/// Live models are stochastic against the mode validators, so a rejected batch is
/// re-requested up to this many times before the run fails loudly. Observed live
/// leak rates for the heavily-constrained modes (implicit negation, hard negative)
/// make three attempts too tight for a 30-batch all-or-nothing run.
pub const MAX_GENERATION_BATCH_ATTEMPTS: usize = 5;

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
pub(crate) const HARD_NEGATIVE_FORBIDDEN_WORDS: &[&str] = &[
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
    "press",
    "pressed",
    "pressing",
    "reluctant",
    "reluctance",
    "dig",
    "digging",
];
pub(crate) const VAGUE_NONE_OF_ROSTER_RETENTION_WORDS: &[&str] = &[
    "remember",
    "remembers",
    "remembered",
    "remembering",
    "remind",
    "reminds",
    "reminded",
    "reminder",
    "reminders",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationRun {
    pub records: Vec<GenerationOutput>,
    pub usage_by_model: Option<BTreeMap<String, TokenUsage>>,
    pub cluster_anchors: Vec<GenerationClusterAnchor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationAnchorRun {
    pub cluster_anchors: Vec<GenerationClusterAnchor>,
    pub usage_by_model: Option<BTreeMap<String, TokenUsage>>,
}

/// Operator-only evidence for the semantic proposition shared by a paraphrase batch.
/// This intentionally never enters the generation-output interchange artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationClusterAnchor {
    pub cluster_id: String,
    pub anchor: String,
}

/// A required scenario shape for a paraphrase batch. These directives deliberately
/// diversify cluster anchors while preserving within-cluster paraphrase coherence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClusterScenarioDirective {
    pub stance: &'static str,
    pub speaker_role: &'static str,
    pub action: &'static str,
    pub consequence: &'static str,
}

const CLUSTER_SCENARIO_DIRECTIVES: [ClusterScenarioDirective; 10] = [
    ClusterScenarioDirective {
        stance: "the speaker sets a boundary of their own",
        speaker_role: "the user as the person who controls access to their own information",
        action: "the speaker states a limit and offers a concrete alternative topic or arrangement",
        consequence: "the other person knows what participation is welcome going forward",
    },
    ClusterScenarioDirective {
        stance: "the speaker respects another person's stated limit",
        speaker_role: "the user as a colleague or friend receiving that limit",
        action: "the speaker changes their own planned action to honor the limit",
        consequence: "the other person can continue on terms they chose",
    },
    ClusterScenarioDirective {
        stance: "the speaker pushes back on a third person who is pressuring someone else",
        speaker_role: "the user as a witness, organizer, or ally",
        action: "the speaker interrupts the pressure and redirects the third person away from the target",
        consequence: "the target is spared having to defend their information alone",
    },
    ClusterScenarioDirective {
        stance: "the speaker seeks advice before an upcoming conversation",
        speaker_role: "the user preparing to speak with another person",
        action: "the speaker asks the AI how to raise a topic while honoring the other person's known limit",
        consequence: "the upcoming conversation can start without putting the other person on the spot",
    },
    ClusterScenarioDirective {
        stance: "the speaker keeps a group exchange within information voluntarily offered",
        speaker_role: "the user facilitating a group or shared discussion",
        action: "the speaker sets the scope of the group discussion around the information already shared",
        consequence: "the group can make its next decision without speculating about anyone's personal details",
    },
    ClusterScenarioDirective {
        stance: "the speaker repairs a prior overstep and takes responsibility",
        speaker_role: "the user as someone who earlier asked more than another person wanted to share",
        action: "the speaker apologizes for the earlier pressure and names the different approach they will take now",
        consequence: "the other person visibly relaxes and the exchange resumes",
    },
    ClusterScenarioDirective {
        stance: "the speaker protects information entrusted by an absent person",
        speaker_role: "the user as custodian of information shared in confidence by someone absent",
        action: "the speaker declines to relay the entrusted information and offers a non-sensitive alternative",
        consequence: "the absent person's information stays where it was entrusted",
    },
    ClusterScenarioDirective {
        stance: "the speaker responds proactively to another person's discomfort",
        speaker_role: "the user as a conversation partner noticing nonverbal unease",
        action: "the speaker changes the subject before anyone has to ask",
        consequence: "the discomfort never has to become a refusal",
    },
    ClusterScenarioDirective {
        stance: "the speaker teaches another person how to leave disclosure voluntary",
        speaker_role: "the user as a senior colleague or family member coaching a junior person",
        action: "the speaker explains how to let people volunteer what they want to share",
        consequence: "the learner handles their next conversation differently",
    },
    ClusterScenarioDirective {
        stance: "the speaker independently restrains their own curiosity before anyone declines or states a limit",
        speaker_role: "the user as someone tempted to ask about information that has not been offered",
        action: "the speaker leaves the tempting subject alone and asks an open question about permitted topics",
        consequence: "the relationship stays on volunteered ground; the stated-limit -> stop -> redirect-to-work -> continued-engagement arc is forbidden",
    },
];

/// Returns the deterministic directive for clusters 1–4 and the hard cluster in each partition.
pub fn cluster_scenario_directive(cluster_id: &str) -> Option<&'static ClusterScenarioDirective> {
    let (partition_offset, cluster_name) =
        if let Some(cluster_name) = cluster_id.strip_prefix("validation-") {
            (0, cluster_name)
        } else if let Some(cluster_name) = cluster_id.strip_prefix("test-") {
            (5, cluster_name)
        } else {
            return None;
        };
    let index = if cluster_name == "hard-cluster" {
        4
    } else {
        cluster_name
            .strip_prefix("cluster-")?
            .parse::<usize>()
            .ok()?
            .checked_sub(1)
            .filter(|index| *index < 4)?
    };
    CLUSTER_SCENARIO_DIRECTIVES.get(partition_offset + index)
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
    pub fn generator_model_id(&self) -> &'static str {
        match self {
            Self::ParaphraseCluster { .. }
            | Self::SubjectConfusion
            | Self::HardParaphrase { .. }
            | Self::VagueNoneOfRoster => SEMANTIC_GENERATOR_MODEL_ID,
            _ => GENERATOR_MODEL_ID,
        }
    }

    pub fn anchor_generator_model_id() -> &'static str {
        SEMANTIC_GENERATOR_MODEL_ID
    }

    pub fn request_count(&self) -> usize {
        match self {
            Self::VagueNoneOfRoster => 12,
            _ => 8,
        }
    }
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
            Self::SubjectConfusion => "Write utterances with an attribution dependency. Every utterance must use exactly one of these patterns: (a) one person pressures a second person about a third person's information; (b) quoted reluctance whose referent is initially ambiguous but resolvable on careful reading; or (c) the user asks whether the AI's own earlier question crossed another person's boundary. The item must remain understandable after careful reading, but the reader must have to resolve who asked, who declined, and whose information is at issue. Two-role lines with the shape \"someone was reluctant so I stopped\" are explicitly forbidden.".to_string(),
            Self::PunctuationCasingLoss => {
                "Write ordinary utterances suitable for later punctuation and casing loss.".to_string()
            }
            Self::SyntheticAsr => "Write ordinary utterances suitable for observed ASR-style casing, punctuation, and entity corruption. Every utterance must contain a personal name or a product/company name that can be plausibly mangled by ASR.".to_string(),
            Self::RareHighCost => "Write rare, high-stakes situations relevant to this description. Every utterance must include both a rare or high-stakes setting and a concrete severe consequence of crossing or protecting the boundary, such as loss of employment, safety, custody, legal exposure, medical confidentiality, or severe reputational harm. Naming divorce, finances, therapy, or an ordinary workplace exchange alone is insufficient. One credible severe consequence is enough; avoid stacking multiple legal consequences, so the result stays natural.".to_string(),
            Self::HardParaphrase { .. } => format!(
                "First fix ONE concrete anchor proposition with specific actors, event, stance, and consequence, showing the situation purely through concrete actions and dialogue cues (short answers, a change of topic, attention returning to the task at hand) and never naming the underlying concept directly. Return that proposition in the `anchor` field. Then write every utterance as a wording-level paraphrase of exactly that proposition, preserving its actors, event, stance, and consequence in every line while varying only the wording. Neither the anchor nor any utterance may contain any of these words: {}.",
                HARD_NEGATIVE_FORBIDDEN_WORDS.join(", ")
            ),
            Self::VagueNoneOfRoster => format!(
                "Write vague everyday utterances outside every roster goal, not merely outside a boundaries-related goal. No goal is supplied for this request. Do not express reluctance or refusal to talk. Also forbid requests to stop or limit a recurring conversation topic, even without boundary vocabulary; recognizing or correcting a conclusion drawn from ambiguous evidence; contrasting a statement or appearance with what was actually true or intended; concrete detail about the speaker's own work, projects, manager, or deadlines; relaying or reporting on absent third parties' relationships, breakups, or affairs, even without endorsing them; weighing what someone said against interpretations of what they really meant; and recurring observations about prices, the economy, technology adoption, or world trends. Do not use any of these words: {}.",
                vague_none_of_roster_forbidden_words().collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptRequest {
    pub description: Option<GoalDescription>,
    pub mode: GenerationMode,
    pub count: usize,
    /// Anchors fixed earlier in this run. Only cluster prompts render them as exclusions.
    pub prior_cluster_anchors: Vec<GenerationClusterAnchor>,
    /// An operator-approved anchor for a cluster batch. This selects the two-step response shape.
    pub approved_cluster_anchor: Option<String>,
}

pub fn build_prompt(request: &PromptRequest) -> Result<String, String> {
    if request.count == 0 {
        return Err("generation request count must be positive".to_string());
    }
    let description = goal_description_prompt(&request.mode, request.description.as_ref())?;
    let cluster_guidance = cluster_guidance(
        &request.mode,
        &request.prior_cluster_anchors,
        request.approved_cluster_anchor.is_none(),
    )?;
    let approved_anchor_instruction = match (&request.mode, &request.approved_cluster_anchor) {
        (
            GenerationMode::ParaphraseCluster { .. } | GenerationMode::HardParaphrase { .. },
            Some(anchor),
        ) => format!(
            "The operator-approved anchor proposition is:\n{anchor}\nWrite every utterance as a wording-level paraphrase of exactly that proposition: preserve its actors, event, stance, and consequence in every line, varying only the wording. Do not return an anchor field.\n"
        ),
        (_, Some(_)) => return Err("only cluster batches may use an approved anchor".to_string()),
        _ => String::new(),
    };
    let response_shape =
        if request.mode.cluster_id().is_some() && request.approved_cluster_anchor.is_none() {
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
{}\n{}{}Return JSON only: {}. Do not include labels, metadata, or explanations.",
        request.count,
        request.mode.instruction(),
        description,
        cluster_guidance,
        approved_anchor_instruction,
        response_shape
    ))
}

/// Constructs the isolated first step of operator-approved paraphrase anchoring.
pub fn build_anchor_prompt(request: &PromptRequest) -> Result<String, String> {
    if request.count != 1 {
        return Err("anchor generation request count must be exactly one".to_string());
    }
    if request.mode.cluster_id().is_none() {
        return Err("anchor generation requires a cluster mode".to_string());
    }
    if request.approved_cluster_anchor.is_some() {
        return Err("anchor generation cannot use an approved anchor".to_string());
    }
    let description = goal_description_prompt(&request.mode, request.description.as_ref())?;
    let cluster_guidance = cluster_guidance(&request.mode, &request.prior_cluster_anchors, true)?;
    let forbidden = match request.mode {
        GenerationMode::HardParaphrase { .. } => format!(
            " The anchor may not contain any of these words: {}.",
            HARD_NEGATIVE_FORBIDDEN_WORDS.join(", ")
        ),
        _ => String::new(),
    };
    Ok(format!(
        "Create exactly one concrete anchor proposition for a later human-user utterance batch. Supply no utterances. The proposition must name specific actors, event, stance, and consequence. {}\n{}{} Return JSON only: {{\"anchor\":\"one concrete proposition\"}}. Do not include labels, metadata, explanations, or an utterances field.",
        description, cluster_guidance, forbidden
    ))
}

fn goal_description_prompt(
    mode: &GenerationMode,
    description: Option<&GoalDescription>,
) -> Result<String, String> {
    match (mode, description) {
        (GenerationMode::VagueNoneOfRoster, None) => Ok(String::new()),
        (GenerationMode::VagueNoneOfRoster, Some(_)) => {
            Err("vague none_of_roster generation must not carry a goal description".to_string())
        }
        (_, Some(description)) => Ok(format!(
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
        )),
        (_, None) => Err("goal-conditioned generation requires a description".to_string()),
    }
}

fn cluster_guidance(
    mode: &GenerationMode,
    prior_cluster_anchors: &[GenerationClusterAnchor],
    is_anchor_authoring: bool,
) -> Result<String, String> {
    match mode.cluster_id() {
        Some(cluster_id) => {
            let directive = cluster_scenario_directive(cluster_id).ok_or_else(|| {
                format!("no scenario directive is configured for cluster {cluster_id}")
            })?;
            let anchor_authoring_guidance = if !is_anchor_authoring {
                String::new()
            } else if prior_cluster_anchors.is_empty() {
                "The anchor must use this required combination.\n".to_string()
            } else {
                format!(
                    "The anchor must use this required combination.\nPreviously fixed anchors in this generation run:\n{}\nYour anchor must differ in stance, actors, action, and consequence from all of these exclusion anchors; do not reuse or adapt their scenario skeletons.\n",
                    prior_cluster_anchors
                        .iter()
                        .map(|anchor| format!("- {}: {}", anchor.cluster_id, anchor.anchor))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };
            Ok(format!(
                "Required scenario directive for this cluster:\n- stance: {}\n- speaker role: {}\n- action: {}\n- consequence: {}\n{}",
                directive.stance,
                directive.speaker_role,
                directive.action,
                directive.consequence,
                anchor_authoring_guidance
            ))
        }
        None => Ok(String::new()),
    }
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
    Ok(parse_generation_response_with_anchor(response, context, true)?.records)
}

fn parse_generation_response_with_anchor(
    response: &str,
    context: &GenerationResponseContext,
    requires_cluster_anchor: bool,
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
    if requires_cluster_anchor && context.mode.cluster_id().is_some() && cluster_anchor.is_none() {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelAnchorResponse {
    anchor: String,
    #[serde(default, rename = "utterances")]
    _utterances: Vec<String>,
}

fn parse_generation_anchor_response(
    response: &str,
    mode: &GenerationMode,
) -> Result<String, String> {
    let response: ModelAnchorResponse = serde_json::from_str(response)
        .map_err(|error| format!("invalid generation anchor response: {error}"))?;
    let anchor = response.anchor.trim();
    if anchor.is_empty() {
        return Err("generation anchor response contains an empty anchor".to_string());
    }
    if matches!(mode, GenerationMode::HardParaphrase { .. })
        && contains_hard_negative_forbidden_word(anchor)
    {
        return Err("hard-negative anchor contains a forbidden word".to_string());
    }
    Ok(anchor.to_string())
}

/// Runs the deterministic generation request schedule through the supplied transport.
/// Prompt construction and response parsing remain transport-independent and testable.
pub fn run_generation<T: ModelTransport>(
    transport: &T,
    roster: &RosterSnapshot,
    generation_run_id: &str,
) -> Result<GenerationRun, String> {
    run_generation_with_approved_anchors(transport, roster, generation_run_id, None)
}

/// Runs utterance generation using operator-approved cluster anchors when supplied.
pub fn run_generation_with_approved_anchors<T: ModelTransport>(
    transport: &T,
    roster: &RosterSnapshot,
    generation_run_id: &str,
    approved_anchors: Option<&[GenerationClusterAnchor]>,
) -> Result<GenerationRun, String> {
    if let Some(anchors) = approved_anchors {
        validate_approved_generation_anchors(anchors)?;
    }
    let goal = roster
        .goals
        .first()
        .ok_or_else(|| "roster has no goals".to_string())?;
    let mut records = Vec::new();
    let mut usage_by_model = BTreeMap::new();
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
                count: mode.request_count(),
                prior_cluster_anchors: cluster_anchors.clone(),
                approved_cluster_anchor: mode.cluster_id().and_then(|cluster_id| {
                    approved_anchors.and_then(|anchors| {
                        anchors
                            .iter()
                            .find(|anchor| anchor.cluster_id == cluster_id)
                            .map(|anchor| anchor.anchor.clone())
                    })
                }),
            })?;
            let model_id = mode.generator_model_id();
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
                generator_model_id: model_id.to_string(),
                synthetic_asr_seed: 20260721,
            };
            let mut attempt = 1usize;
            let parsed = loop {
                let response = transport.complete(&CompletionRequest {
                    model_id: model_id.to_string(),
                    prompt: prompt.clone(),
                    goal_ref: goal.goal_ref.clone(),
                    run_id: generation_run_id.to_string(),
                    utterance_id: None,
                })?;
                if let Some(call_usage) = response.usage {
                    add_usage(&mut usage_by_model, model_id, call_usage);
                    has_usage = true;
                }
                match parse_generation_response_with_anchor(
                    &response.content,
                    &context,
                    approved_anchors.is_none(),
                )
                .and_then(|parsed| require_requested_utterance_count(parsed, &mode))
                {
                    Ok(parsed) => break parsed,
                    Err(error) if attempt < MAX_GENERATION_BATCH_ATTEMPTS => {
                        engine_logging::engine_warn!(
                            "goal relevance generation batch rejected, re-requesting run_id={} mode={} model_id={} attempt={}/{} error={}",
                            generation_run_id,
                            mode.name(),
                            model_id,
                            attempt,
                            MAX_GENERATION_BATCH_ATTEMPTS,
                            error
                        );
                        attempt += 1;
                    }
                    Err(error) => {
                        engine_logging::engine_error!(
                            "goal relevance generation rejected run_id={} mode={} model_id={} attempts={} error={}",
                            generation_run_id,
                            mode.name(),
                            model_id,
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
            records.extend(parsed.records.into_iter().take(mode.request_count()));
        }
    }
    Ok(GenerationRun {
        records,
        usage_by_model: has_usage.then_some(usage_by_model),
        cluster_anchors: approved_anchors
            .map(ToOwned::to_owned)
            .unwrap_or(cluster_anchors),
    })
}

fn require_requested_utterance_count(
    parsed: ParsedGenerationResponse,
    mode: &GenerationMode,
) -> Result<ParsedGenerationResponse, String> {
    let delivered = parsed.records.len();
    let requested = mode.request_count();
    if delivered < requested {
        return Err(format!(
            "generation mode {} delivered {delivered} utterances, requested {requested}",
            mode.name()
        ));
    }
    Ok(parsed)
}

/// Runs only the first anchor checkpoint for every scheduled paraphrase cluster.
pub fn run_generation_anchors<T: ModelTransport>(
    transport: &T,
    roster: &RosterSnapshot,
    generation_run_id: &str,
) -> Result<GenerationAnchorRun, String> {
    let goal = roster
        .goals
        .first()
        .ok_or_else(|| "roster has no goals".to_string())?;
    let mut cluster_anchors = Vec::new();
    let mut usage_by_model = BTreeMap::new();
    let mut has_usage = false;
    for mode in cluster_generation_modes() {
        let prompt = build_anchor_prompt(&PromptRequest {
            description: Some(GoalDescription::from(goal)),
            mode: mode.clone(),
            count: 1,
            prior_cluster_anchors: cluster_anchors.clone(),
            approved_cluster_anchor: None,
        })?;
        let cluster_id = mode
            .cluster_id()
            .expect("cluster schedule contains only cluster modes");
        let model_id = GenerationMode::anchor_generator_model_id();
        let mut attempt = 1usize;
        let anchor = loop {
            let response = transport.complete(&CompletionRequest {
                model_id: model_id.to_string(),
                prompt: prompt.clone(),
                goal_ref: goal.goal_ref.clone(),
                run_id: generation_run_id.to_string(),
                utterance_id: None,
            })?;
            if let Some(call_usage) = response.usage {
                add_usage(&mut usage_by_model, model_id, call_usage);
                has_usage = true;
            }
            match parse_generation_anchor_response(&response.content, &mode) {
                Ok(anchor) => break anchor,
                Err(error) if attempt < MAX_GENERATION_BATCH_ATTEMPTS => {
                    engine_logging::engine_warn!(
                        "goal relevance anchor rejected, re-requesting run_id={} mode={} model_id={} attempt={}/{} error={}",
                        generation_run_id,
                        mode.name(),
                        model_id,
                        attempt,
                        MAX_GENERATION_BATCH_ATTEMPTS,
                        error
                    );
                    attempt += 1;
                }
                Err(error) => {
                    engine_logging::engine_error!(
                        "goal relevance anchor rejected run_id={} mode={} model_id={} attempts={} error={}",
                        generation_run_id,
                        mode.name(),
                        model_id,
                        attempt,
                        error
                    );
                    return Err(error);
                }
            }
        };
        cluster_anchors.push(GenerationClusterAnchor {
            cluster_id: cluster_id.to_string(),
            anchor,
        });
    }
    Ok(GenerationAnchorRun {
        cluster_anchors,
        usage_by_model: has_usage.then_some(usage_by_model),
    })
}

fn add_usage(usage_by_model: &mut BTreeMap<String, TokenUsage>, model_id: &str, usage: TokenUsage) {
    usage_by_model
        .entry(model_id.to_string())
        .or_insert(TokenUsage {
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
        })
        .add(usage);
}

/// Parses the operator-only cluster-anchor evidence sidecar. It is intentionally
/// separate from the generation-output interchange artifact.
pub fn parse_generation_anchor_sidecar(
    input: &str,
) -> Result<Vec<GenerationClusterAnchor>, String> {
    let anchors = crate::parse_jsonl(input, "generation anchor sidecar")?;
    validate_generation_anchor_sidecar(&anchors)?;
    Ok(anchors)
}

/// Serializes the operator-only cluster-anchor evidence sidecar.
pub fn write_generation_anchor_sidecar(
    anchors: &[GenerationClusterAnchor],
) -> Result<String, String> {
    validate_generation_anchor_sidecar(anchors)?;
    crate::write_jsonl(anchors)
}

/// Validates the standalone anchor evidence without imposing any interchange contract.
pub fn validate_generation_anchor_sidecar(
    anchors: &[GenerationClusterAnchor],
) -> Result<(), String> {
    let mut cluster_ids = BTreeSet::new();
    for anchor in anchors {
        if anchor.cluster_id.trim().is_empty() {
            return Err("generation anchor sidecar contains an empty cluster_id".to_string());
        }
        if anchor.anchor.trim().is_empty() {
            return Err(format!(
                "generation anchor sidecar contains an empty anchor for cluster {}",
                anchor.cluster_id
            ));
        }
        if !cluster_ids.insert(&anchor.cluster_id) {
            return Err(format!(
                "generation anchor sidecar contains duplicate cluster_id {}",
                anchor.cluster_id
            ));
        }
    }
    Ok(())
}

/// Validates an operator-approved checkpoint: exactly the scheduled ten cluster anchors,
/// no more and no less.
pub fn validate_approved_generation_anchors(
    anchors: &[GenerationClusterAnchor],
) -> Result<(), String> {
    validate_generation_anchor_sidecar(anchors)?;
    let actual = anchors
        .iter()
        .map(|anchor| anchor.cluster_id.as_str())
        .collect::<BTreeSet<_>>();
    let expected_ids = expected_cluster_ids();
    let expected = expected_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let extra = actual.difference(&expected).copied().collect::<Vec<_>>();
        return Err(format!(
            "approved generation anchors must contain exactly one entry for every scheduled cluster; missing [{}], extra [{}]",
            missing.join(", "),
            extra.join(", ")
        ));
    }
    Ok(())
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

fn cluster_generation_modes() -> Vec<GenerationMode> {
    ["validation", "test"]
        .into_iter()
        .flat_map(|partition| {
            let mut modes = (1..=4)
                .map(|cluster| GenerationMode::ParaphraseCluster {
                    cluster_id: format!("{partition}-cluster-{cluster}"),
                })
                .collect::<Vec<_>>();
            modes.push(GenerationMode::HardParaphrase {
                cluster_id: format!("{partition}-hard-cluster"),
            });
            modes
        })
        .collect()
}

fn expected_cluster_ids() -> Vec<String> {
    cluster_generation_modes()
        .into_iter()
        .map(|mode| {
            mode.cluster_id()
                .expect("cluster schedule contains only cluster modes")
                .to_string()
        })
        .collect()
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
        GenerationMode::VagueNoneOfRoster => records
            .iter()
            .filter(|record| contains_vague_none_of_roster_forbidden_word(&record.utterance))
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
    contains_forbidden_word(
        &normalized_words(utterance),
        HARD_NEGATIVE_FORBIDDEN_WORDS.iter().copied(),
    )
}

fn contains_vague_none_of_roster_forbidden_word(utterance: &str) -> bool {
    contains_forbidden_word(
        &normalized_words(utterance),
        vague_none_of_roster_forbidden_words(),
    )
}

fn vague_none_of_roster_forbidden_words() -> impl Iterator<Item = &'static str> {
    VAGUE_NONE_OF_ROSTER_RETENTION_WORDS
        .iter()
        .copied()
        .chain(HARD_NEGATIVE_FORBIDDEN_WORDS.iter().copied())
}

fn contains_forbidden_word<'a>(
    words: &[String],
    forbidden_words: impl IntoIterator<Item = &'a str>,
) -> bool {
    forbidden_words
        .into_iter()
        .any(|forbidden| words.iter().any(|word| word == forbidden))
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
