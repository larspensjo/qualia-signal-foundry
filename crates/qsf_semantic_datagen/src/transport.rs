use std::{env, fs, path::PathBuf};

use openai_provider_kit::{
    ChatMessage, ChatRole, LlmProvider, LlmRequest, ModelId, OpenAiProvider, ProviderKind,
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

pub trait ModelTransport {
    fn complete(&self, request: &CompletionRequest) -> Result<String, String>;
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
    fn complete(&self, _request: &CompletionRequest) -> Result<String, String> {
        Self::default_response(self.response)
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
    fn complete(&self, request: &CompletionRequest) -> Result<String, String> {
        let provider_request = LlmRequest::new(
            ModelId::new(ProviderKind::OpenAi, request.model_id.clone()),
            vec![ChatMessage::new(ChatRole::User, request.prompt.clone())],
        )
        .with_json_response();
        self.runtime
            .block_on(self.provider.complete(&provider_request))
            .map(|response| response.content().to_string())
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
            let mut validated_records = 0;
            for fixture in [
                FixtureResponse::Generation,
                FixtureResponse::MiniLabel,
                FixtureResponse::FableLabel,
            ] {
                let request = CompletionRequest {
                    model_id: match fixture {
                        FixtureResponse::Generation => "gpt-5.4-nano",
                        FixtureResponse::MiniLabel => "gpt-5.4-mini",
                        FixtureResponse::FableLabel => "claude-fable",
                    }
                    .to_string(),
                    prompt: "checked-in replay fixture".to_string(),
                    goal_ref: "replay-roster".to_string(),
                    run_id: "replay-validation".to_string(),
                };
                let response = ReplayTransport::new(fixture).complete(&request)?;
                validated_records += match fixture {
                    FixtureResponse::Generation => crate::parse_generation_output(&response)?.len(),
                    FixtureResponse::MiniLabel | FixtureResponse::FableLabel => {
                        crate::parse_label_interchange(&response, &roster)?.len()
                    }
                };
            }
            println!("using replay transport ({validated_records} fixture records validated)");
            Ok(())
        }
        TransportKind::Live => {
            engine_logging::engine_info!(
                "goal relevance datagen live transport selected model_id=gpt-5.4-nano run_id=manual"
            );
            println!("using live transport");
            Ok(())
        }
    }
}
