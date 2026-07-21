use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use openai_provider_kit::{
    ChatMessage, ChatRole, LlmProvider, LlmRequest, ModelId, OpenAiProvider, ProviderKind,
};
use qsf_semantic_eval::{GoldLabel, RosterSnapshot};

use crate::{
    GENERATOR_MODEL_ID, GOAL_RELEVANCE_GUIDELINE_VERSION, LabelingResponseContext, MINI_LABELER_ID,
    ReviewDecision, ReviewField, ReviewValue, TokenUsage, build_blind_qa_review_view_model,
    build_labeling_input, build_review_view_model, mini_fable_agreement_rate,
    parse_generation_output, parse_goal_relevance_label_response, parse_label_interchange,
    parse_labeling_input, parse_review_decisions, priority_review_queue,
    priority_review_utterance_ids, reconcile, render_blind_qa_review_view, render_review_view,
    render_usage_report, run_generation, run_mini_labeling, split_feasibility_preflight,
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
    pub utterance_id: Option<String>,
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
                    "goal relevance datagen completion failed goal_ref={} utterance_id={} run_id={} model_id={}: {error}",
                    request.goal_ref,
                    request.utterance_id.as_deref().unwrap_or("n/a"),
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

enum SelectedTransport {
    Replay(ReplayTransport),
    Live(LiveTransport),
}

impl SelectedTransport {
    fn kind(&self) -> TransportKind {
        match self {
            Self::Replay(_) => TransportKind::Replay,
            Self::Live(_) => TransportKind::Live,
        }
    }
}

impl ModelTransport for SelectedTransport {
    fn complete(&self, request: &CompletionRequest) -> Result<Completion, String> {
        match self {
            Self::Replay(transport) => transport.complete(request),
            Self::Live(transport) => transport.complete(request),
        }
    }
}

fn selected_transport(
    live: bool,
    replay_response: FixtureResponse,
) -> Result<SelectedTransport, String> {
    if live {
        LiveTransport::from_env().map(SelectedTransport::Live)
    } else {
        Ok(SelectedTransport::Replay(ReplayTransport::new(
            replay_response,
        )))
    }
}
pub fn run_cli(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let args = args.by_ref().collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        None => run_replay_labeling_smoke(),
        Some("generate") => run_generate_cli(&args[1..]),
        Some("label") => run_label_cli(&args[1..]),
        Some("reconcile") => run_reconcile_cli(&args[1..]),
        Some("validate-labels") => run_validate_labels_cli(&args[1..]),
        Some("review") => run_review_cli(&args[1..]),
        _ => Err(usage().to_string()),
    }
}

fn usage() -> &'static str {
    "usage:\n  qsf_semantic_datagen generate [--live] <roster.json> <generation-output.jsonl>\n  qsf_semantic_datagen label [--live] <generation-output.jsonl> <roster.json> <output-dir> [labeling-run-id]\n  qsf_semantic_datagen reconcile <roster.json> <label-mini.jsonl> <label-fable.jsonl> <reconciliation.jsonl>\n  qsf_semantic_datagen validate-labels <roster.json> <label-interchange.jsonl>\n  qsf_semantic_datagen review [--blind-qa] <roster.json> <labeling-input.jsonl> <label-mini.jsonl> <label-fable.jsonl> <review-decisions.jsonl>"
}

fn run_replay_labeling_smoke() -> Result<(), String> {
    let roster = default_roster()?;
    let generated = run_generation(
        &ReplayTransport::new(FixtureResponse::Generation),
        &roster,
        "fixture-generation-run",
    )?
    .records;
    validate_generation_output(&generated)?;
    let feasibility = split_feasibility_preflight(&generated, 20260721)?;
    let input = build_labeling_input(&generated[..1], &roster)?;
    let mini = run_mini_labeling(
        &ReplayTransport::new(FixtureResponse::MiniLabel),
        &input,
        "fixture-mini-run",
    )?;
    let mini_json = write_jsonl(&mini.labels)?;
    let mini = parse_label_interchange(&mini_json, &roster)?;
    let fable_response = ReplayTransport::new(FixtureResponse::FableLabel).complete(
        &replay_request("blind-full-roster", "fixture-fable-run", "claude-fable"),
    )?;
    let fable = parse_goal_relevance_label_response(
        &fable_response.content,
        LabelingResponseContext {
            input: &input[0],
            labeler_id: "claude-fable",
            labeling_run_id: "fixture-fable-run",
            guideline_version: GOAL_RELEVANCE_GUIDELINE_VERSION,
        },
    )?;
    let fable_json = write_jsonl(&[fable])?;
    let fable = parse_label_interchange(&fable_json, &roster)?;
    let reconciliation = reconcile(&mini, &fable)?;
    let agreement = mini_fable_agreement_rate(&reconciliation);
    println!(
        "using replay transport (mini labeling fixture completed; {}/{} agreement; {} split components assigned)",
        agreement.agreed_pairs,
        agreement.total_pairs,
        feasibility.assignment_by_component.len()
    );
    Ok(())
}

fn run_generate_cli(args: &[String]) -> Result<(), String> {
    let (live, args) = live_flag(args);
    if args.len() != 2 {
        return Err(usage().to_string());
    }
    let roster = RosterSnapshot::from_json_path(&args[0])?;
    let run_id = if live {
        "goalrel-generation-live"
    } else {
        "goalrel-generation-replay"
    };
    let transport = selected_transport(live, FixtureResponse::Generation)?;
    if transport.kind() == TransportKind::Live {
        engine_logging::engine_info!(
            "goal relevance generation live transport selected run_id={} model_id={}",
            run_id,
            GENERATOR_MODEL_ID
        );
    }
    let run = run_generation(&transport, &roster, run_id)?;
    validate_generation_output(&run.records)?;
    let feasibility = split_feasibility_preflight(&run.records, 20260721)?;
    let output_path = Path::new(&args[1]);
    write_file(output_path, &write_jsonl(&run.records)?)?;
    println!(
        "wrote {} generated utterances across {} split components to {} using {} transport",
        run.records.len(),
        feasibility.assignment_by_component.len(),
        output_path.display(),
        transport_name(transport.kind())
    );
    for cluster in &run.cluster_anchors {
        println!(
            "paraphrase cluster {} anchor: {}",
            cluster.cluster_id, cluster.anchor
        );
    }
    if let Some(usage) = run.usage {
        println!("{}", render_usage_report(GENERATOR_MODEL_ID, usage));
    } else {
        println!("generation token usage unavailable from replay transport");
    }
    Ok(())
}

fn run_label_cli(args: &[String]) -> Result<(), String> {
    let (live, args) = live_flag(args);
    if !(3..=4).contains(&args.len()) {
        return Err(usage().to_string());
    }
    let generated = parse_generation_output(&read_file(&args[0])?)?;
    let roster = qsf_semantic_eval::RosterSnapshot::from_json_path(&args[1])?;
    let input = build_labeling_input(&generated, &roster)?;
    let run_id = args.get(3).map(String::as_str).unwrap_or("mini-manual-run");
    let transport = selected_transport(live, FixtureResponse::MiniLabel)?;
    if transport.kind() == TransportKind::Live {
        engine_logging::engine_info!(
            "goal relevance mini labeling live transport selected run_id={} model_id={}",
            run_id,
            MINI_LABELER_ID
        );
    }
    let run = run_mini_labeling(&transport, &input, run_id)?;
    let output_dir = Path::new(&args[2]);
    fs::create_dir_all(output_dir).map_err(|error| {
        format!(
            "could not create output directory {}: {error}",
            output_dir.display()
        )
    })?;
    write_file(
        output_dir.join("labeling-input.jsonl"),
        &write_jsonl(&input)?,
    )?;
    let labels_json = write_jsonl(&run.labels)?;
    let labels = parse_label_interchange(&labels_json, &roster)?;
    write_file(output_dir.join("label-mini.jsonl"), &labels_json)?;
    println!(
        "wrote {} labeling inputs and {} mini labels to {} using {} transport",
        input.len(),
        labels.len(),
        output_dir.display(),
        transport_name(transport.kind())
    );
    if let Some(usage) = run.usage {
        println!("{}", render_usage_report(MINI_LABELER_ID, usage));
    } else {
        println!("mini labeling token usage unavailable from replay transport");
    }
    Ok(())
}

fn run_reconcile_cli(args: &[String]) -> Result<(), String> {
    if args.len() != 4 {
        return Err(usage().to_string());
    }
    let roster = qsf_semantic_eval::RosterSnapshot::from_json_path(&args[0])?;
    let mini = parse_label_interchange(&read_file(&args[1])?, &roster)?;
    let fable = parse_label_interchange(&read_file(&args[2])?, &roster)?;
    let reconciliation = reconcile(&mini, &fable)?;
    write_file(Path::new(&args[3]), &write_jsonl(&reconciliation)?)?;
    let agreement = mini_fable_agreement_rate(&reconciliation);
    println!(
        "wrote {} reconciliation pairs; mini/Fable agreement {}/{} ({:.2}%)",
        reconciliation.len(),
        agreement.agreed_pairs,
        agreement.total_pairs,
        agreement.rate * 100.0
    );
    Ok(())
}

fn run_validate_labels_cli(args: &[String]) -> Result<(), String> {
    if args.len() != 2 {
        return Err(usage().to_string());
    }
    let roster = qsf_semantic_eval::RosterSnapshot::from_json_path(&args[0])?;
    let labels = parse_label_interchange(&read_file(&args[1])?, &roster)?;
    println!("validated {} label interchange record(s)", labels.len());
    Ok(())
}

fn run_review_cli(args: &[String]) -> Result<(), String> {
    let (blind_qa, args) = match args.first().map(String::as_str) {
        Some("--blind-qa") => (true, &args[1..]),
        _ => (false, args),
    };
    if args.len() != 5 {
        return Err(usage().to_string());
    }
    let roster = qsf_semantic_eval::RosterSnapshot::from_json_path(&args[0])?;
    let input = parse_labeling_input(&read_file(&args[1])?)?;
    let mini = parse_label_interchange(&read_file(&args[2])?, &roster)?;
    let fable = parse_label_interchange(&read_file(&args[3])?, &roster)?;
    let review_decisions_path = Path::new(&args[4]);
    let existing = if review_decisions_path.exists() {
        parse_review_decisions(&read_file(review_decisions_path)?)?
    } else {
        Vec::new()
    };
    let output_path = review_output_path(review_decisions_path, blind_qa);
    let reconciliation = reconcile(&mini, &fable)?;
    let queue = priority_review_queue(&reconciliation);
    println!(
        "reviewing {} pairs ({} disagreements first)",
        queue.len(),
        queue.iter().filter(|entry| !entry.agree).count()
    );
    let mut append = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&output_path)
        .map_err(|error| {
            format!(
                "could not open {} for append: {error}",
                output_path.display()
            )
        })?;
    for utterance_id in priority_review_utterance_ids(&reconciliation) {
        let view = build_review_view_model(&utterance_id, &input, &mini, &fable, &existing)?;
        if blind_qa {
            println!(
                "{}",
                render_blind_qa_review_view(&build_blind_qa_review_view_model(
                    &utterance_id,
                    &input
                )?)
            );
        } else {
            println!("{}", render_review_view(&view));
        }
        let decisions = prompt_review_decisions(&view, blind_qa)?;
        for decision in decisions {
            let line = serde_json::to_string(&decision).map_err(|error| error.to_string())?;
            writeln!(append, "{line}").map_err(|error| {
                format!(
                    "could not append review decision to {}: {error}",
                    output_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn prompt_review_decisions(
    view: &crate::ReviewViewModel,
    blind_qa: bool,
) -> Result<Vec<ReviewDecision>, String> {
    let mut decisions = Vec::with_capacity(view.per_goal.len() + 1);
    for goal in &view.per_goal {
        let default = if blind_qa { None } else { Some(&goal.label) };
        let label = prompt_label(&goal.title, default)?;
        decisions.push(ReviewDecision {
            decided_at: operator_timestamp()?,
            utterance_id: view.utterance_id.clone(),
            goal_ref: Some(goal.goal_ref.clone()),
            field: ReviewField::GoldLabel,
            value: ReviewValue::GoldLabel(label),
        });
    }
    let default = if blind_qa {
        None
    } else {
        Some(view.none_of_roster)
    };
    let none_of_roster = loop {
        let answer = prompt_none_of_roster(default)?;
        match review_none_of_roster_action(&decisions, answer) {
            NoneOfRosterAction::Accept(value) => break value,
            NoneOfRosterAction::Reprompt => eprintln!(
                "none_of_roster cannot be yes while a goal is Relevant; answer no or restart this utterance to revise the goal label"
            ),
        }
    };
    decisions.push(ReviewDecision {
        decided_at: operator_timestamp()?,
        utterance_id: view.utterance_id.clone(),
        goal_ref: None,
        field: ReviewField::NoneOfRoster,
        value: ReviewValue::NoneOfRoster(none_of_roster),
    });
    Ok(decisions)
}

fn prompt_label(title: &str, default: Option<&GoldLabel>) -> Result<GoldLabel, String> {
    loop {
        let default_text = default
            .map(|label| format!("; Enter accepts {:?}", label))
            .unwrap_or_default();
        print!(
            "{} [r=Relevant, n=NotRelevant, a=Ambiguous{}]: ",
            title, default_text
        );
        io::stdout().flush().map_err(|error| error.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|error| error.to_string())?;
        match answer.trim().to_ascii_lowercase().as_str() {
            "r" | "relevant" => return Ok(GoldLabel::Relevant),
            "n" | "not_relevant" => return Ok(GoldLabel::NotRelevant),
            "a" | "ambiguous" => return Ok(GoldLabel::Ambiguous),
            "" if default.is_some() => return Ok(default.expect("checked").clone()),
            _ => eprintln!("enter r, n, or a"),
        }
    }
}

fn prompt_none_of_roster(default: Option<bool>) -> Result<bool, String> {
    loop {
        let default_text = default
            .map(|value| format!("; Enter accepts {value}"))
            .unwrap_or_default();
        print!("none_of_roster [y/n{}]: ", default_text);
        io::stdout().flush().map_err(|error| error.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|error| error.to_string())?;
        match answer.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            "" if default.is_some() => return Ok(default.expect("checked")),
            _ => eprintln!("enter y or n"),
        }
    }
}

fn operator_timestamp() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock precedes Unix epoch: {error}"))?
        .as_secs();
    Ok(format!("operator-cli-unix-{seconds}"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NoneOfRosterAction {
    Accept(bool),
    Reprompt,
}

pub(crate) fn review_none_of_roster_action(
    decisions: &[ReviewDecision],
    answer: bool,
) -> NoneOfRosterAction {
    if answer
        && decisions
            .iter()
            .any(|decision| matches!(decision.value, ReviewValue::GoldLabel(GoldLabel::Relevant)))
    {
        NoneOfRosterAction::Reprompt
    } else {
        NoneOfRosterAction::Accept(answer)
    }
}

pub(crate) fn review_output_path(review_decisions_path: &Path, blind_qa: bool) -> PathBuf {
    if blind_qa {
        review_decisions_path.with_file_name("blind-qa-decisions.jsonl")
    } else {
        review_decisions_path.to_path_buf()
    }
}

fn live_flag(args: &[String]) -> (bool, &[String]) {
    match args.first().map(String::as_str) {
        Some("--live") => (true, &args[1..]),
        _ => (false, args),
    }
}

fn transport_name(kind: TransportKind) -> &'static str {
    match kind {
        TransportKind::Replay => "replay",
        TransportKind::Live => "live",
    }
}

fn default_roster() -> Result<RosterSnapshot, String> {
    let roster_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../evaluation/frozen/goal-relevance/realtime-seed.roster.json");
    RosterSnapshot::from_json_path(roster_path)
}

fn read_file(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

fn write_file(path: impl AsRef<Path>, content: &str) -> Result<(), String> {
    let path = path.as_ref();
    fs::write(path, content).map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn replay_request(goal_ref: &str, run_id: &str, model_id: &str) -> CompletionRequest {
    CompletionRequest {
        model_id: model_id.to_string(),
        prompt: "checked-in replay fixture".to_string(),
        goal_ref: goal_ref.to_string(),
        run_id: run_id.to_string(),
        utterance_id: None,
    }
}
