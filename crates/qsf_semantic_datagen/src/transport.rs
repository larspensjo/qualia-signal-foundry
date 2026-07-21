use std::{env, fs, path::PathBuf};

use openai_provider_kit::{
    ChatMessage, ChatRole, LlmProvider, LlmRequest, ModelId, OpenAiProvider, ProviderKind,
};
use qsf_semantic_eval::RosterSnapshot;

use crate::{
    GenerationMode, GenerationResponseContext, GoalDescription, PromptRequest, TokenUsage,
    build_prompt, parse_generation_response, render_usage_report, split_feasibility_preflight,
    validate_generation_output, write_jsonl,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportKind {
    Replay,
    Live,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureResponse {
    Generation,
    MiniLabel,
    FableLabel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionRequest {
    pub model_id: String,
    pub prompt: String,
    pub goal_ref: String,
    pub run_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    pub content: String,
    pub usage: Option<TokenUsage>,
}

pub trait ModelTransport {
    fn complete(&self, request: &CompletionRequest) -> Result<Completion, String>;
}

pub struct ReplayTransport {
    response: FixtureResponse,
}
impl ReplayTransport {
    pub fn new(response: FixtureResponse) -> Self {
        Self { response }
    }

    pub fn default_response(kind: FixtureResponse) -> Result<String, String> {
        let path = match kind {
            FixtureResponse::Generation => "fixtures/generation-response.json",
            FixtureResponse::MiniLabel => "fixtures/mini-label-response.json",
            FixtureResponse::FableLabel => "fixtures/fable-label-response.json",
        };
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
        fs::read_to_string(&fixture).map_err(|error| {
            format!(
                "could not read replay fixture {}: {error}",
                fixture.display()
            )
        })
    }
}
impl ModelTransport for ReplayTransport {
    fn complete(&self, _request: &CompletionRequest) -> Result<Completion, String> {
        Self::default_response(self.response).map(|content| Completion {
            content,
            usage: None,
        })
    }
}

pub struct LiveTransport {
    provider: OpenAiProvider,
    runtime: tokio::runtime::Runtime,
}
impl LiveTransport {
    pub fn from_env() -> Result<Self, String> {
        if env::var("OPENAI_API_KEY").is_err() {
            return Err("--live requires OPENAI_API_KEY".to_string());
        }
        let provider = OpenAiProvider::from_env()
            .map_err(|error| format!("could not initialize OpenAI provider: {error}"))?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("could not build Tokio runtime: {error}"))?;
        Ok(Self { provider, runtime })
    }
}
impl ModelTransport for LiveTransport {
    fn complete(&self, request: &CompletionRequest) -> Result<Completion, String> {
        let provider_request = LlmRequest::new(
            ModelId::new(ProviderKind::OpenAi, request.model_id.clone()),
            vec![ChatMessage::new(ChatRole::User, request.prompt.clone())],
        )
        .with_json_response();
        self.runtime
            .block_on(self.provider.complete(&provider_request))
            .map(|response| {
                let usage = response.usage();
                Completion {
                    content: response.content().to_string(),
                    usage: Some(TokenUsage {
                        input_tokens: usage.input_tokens,
                        cached_input_tokens: usage.cached_input_tokens,
                        output_tokens: usage.output_tokens,
                    }),
                }
            })
            .map_err(|error| {
                engine_logging::engine_error!(
                    "goal relevance datagen completion failed goal_ref={} run_id={} model_id={}: {error}",
                    request.goal_ref,
                    request.run_id,
                    request.model_id
                );
                format!("live model completion failed model_id={}: {error}", request.model_id)
            })
    }
}

pub fn default_transport_kind() -> TransportKind {
    TransportKind::Replay
}
pub fn selected_transport(live: bool) -> Result<TransportKind, String> {
    if live {
        LiveTransport::from_env().map(|_| TransportKind::Live)
    } else {
        Ok(default_transport_kind())
    }
}
pub fn run_cli(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let live = match args.next().as_deref() {
        None => false,
        Some("--live") => true,
        Some(value) => {
            return Err(format!(
                "unknown argument {value}; usage: qsf_semantic_datagen [--live]"
            ));
        }
    };
    if args.next().is_some() {
        return Err("usage: qsf_semantic_datagen [--live]".to_string());
    }
    match selected_transport(live)? {
        TransportKind::Replay => {
            let roster_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json");
            let roster = qsf_semantic_eval::RosterSnapshot::from_json_path(roster_path)?;
            let generated = replay_generation_pool(&roster)?;
            validate_generation_output(&generated)?;
            let feasibility = split_feasibility_preflight(&generated, 20260721)?;
            let mut validated_records = generated.len();
            for fixture in [FixtureResponse::MiniLabel, FixtureResponse::FableLabel] {
                let response = ReplayTransport::new(fixture).complete(&replay_request(
                    "replay-roster",
                    "replay-validation",
                    "fixture",
                ))?;
                validated_records +=
                    crate::parse_label_interchange(&response.content, &roster)?.len();
            }
            println!(
                "using replay transport ({validated_records} fixture records validated; {} split components assigned)",
                feasibility.assignment_by_component.len()
            );
            Ok(())
        }
        TransportKind::Live => {
            engine_logging::engine_info!(
                "goal relevance datagen live transport selected model_id=gpt-5.4-nano run_id=manual"
            );
            let roster_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json");
            let roster = qsf_semantic_eval::RosterSnapshot::from_json_path(roster_path)?;
            let goal = roster
                .goals
                .first()
                .ok_or_else(|| "roster has no goals".to_string())?;
            let prompt = build_prompt(&PromptRequest {
                description: Some(GoalDescription::from(goal)),
                mode: GenerationMode::Natural,
                count: 5,
            })?;
            let request = CompletionRequest {
                model_id: "gpt-5.4-nano".to_string(),
                prompt,
                goal_ref: goal.goal_ref.clone(),
                run_id: "goalrel-live-smoke".to_string(),
            };
            let response = LiveTransport::from_env()?.complete(&request)?;
            let generated = parse_generation_response(
                &response.content,
                &GenerationResponseContext {
                    utterance_id_prefix: "live-smoke".to_string(),
                    language: "en".to_string(),
                    conditioning_goal_ref: Some(goal.goal_ref.clone()),
                    mode: GenerationMode::Natural,
                    session_id: "live-smoke-session".to_string(),
                    semantic_cluster_id: "live-smoke-cluster".to_string(),
                    generation_run_id: request.run_id.clone(),
                    generator_model_id: request.model_id.clone(),
                    synthetic_asr_seed: 20260721,
                },
            )?;
            println!(
                "using live transport ({} utterances generated)",
                generated.len()
            );
            println!("{}", write_jsonl(&generated)?);
            if let Some(usage) = response.usage {
                println!("{}", render_usage_report(&request.model_id, usage));
            }
            Ok(())
        }
    }
}

fn replay_request(goal_ref: &str, run_id: &str, model_id: &str) -> CompletionRequest {
    CompletionRequest {
        model_id: model_id.to_string(),
        prompt: "checked-in replay fixture".to_string(),
        goal_ref: goal_ref.to_string(),
        run_id: run_id.to_string(),
    }
}

fn replay_generation_pool(roster: &RosterSnapshot) -> Result<Vec<crate::GenerationOutput>, String> {
    let goal = roster
        .goals
        .first()
        .ok_or_else(|| "roster has no goals".to_string())?;
    let transport = ReplayTransport::new(FixtureResponse::Generation);
    let mut generated = Vec::new();
    for partition in ["validation", "test"] {
        let mut modes = vec![GenerationMode::Natural];
        for cluster in 1..=4 {
            modes.push(GenerationMode::ParaphraseCluster {
                cluster_id: format!("fixture-{partition}-cluster-{cluster}"),
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
                cluster_id: format!("fixture-{partition}-hard-cluster"),
            },
            GenerationMode::VagueNoneOfRoster,
        ]);
        for (index, mode) in modes.iter().enumerate() {
            let description = if *mode == GenerationMode::VagueNoneOfRoster {
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
                model_id: "gpt-5.4-nano".to_string(),
                prompt,
                goal_ref: goal.goal_ref.clone(),
                run_id: "fixture-generation-run".to_string(),
            })?;
            generated.extend(parse_generation_response(
                &response.content,
                &GenerationResponseContext {
                    utterance_id_prefix: format!("fixture-{partition}-{index}"),
                    language: "en".to_string(),
                    conditioning_goal_ref: if *mode == GenerationMode::VagueNoneOfRoster {
                        None
                    } else {
                        Some(goal.goal_ref.clone())
                    },
                    mode: mode.clone(),
                    session_id: format!("fixture-session-{partition}"),
                    semantic_cluster_id: format!("fixture-semantic-{partition}"),
                    generation_run_id: "fixture-generation-run".to_string(),
                    generator_model_id: "gpt-5.4-nano".to_string(),
                    synthetic_asr_seed: 20260721,
                },
            )?);
        }
    }
    Ok(generated)
}
