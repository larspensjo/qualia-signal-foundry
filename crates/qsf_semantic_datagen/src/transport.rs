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
    AgreementEvidence, FreezeRequest, GENERATOR_MODEL_ID, GOAL_RELEVANCE_GUIDELINE_VERSION,
    GatekeeperRequest, LabelingResponseContext, MINI_LABELER_ID, ReviewDecision, ReviewField,
    ReviewValue, SEMANTIC_GENERATOR_MODEL_ID, TokenUsage, build_blind_qa_review_view_model,
    build_labeling_input, build_review_view_model, freeze_artifacts,
    gatekeep_goal_relevance_freeze, methodology_goal_relevance, mini_fable_agreement_rate,
    parse_freeze_manifest, parse_generation_anchor_sidecar, parse_generation_output,
    parse_goal_relevance_label_response, parse_label_interchange, parse_labeling_input,
    parse_reconciliation, parse_review_decisions, parse_reviewed_pool, parse_split_summary,
    priority_review_queue, priority_review_utterance_ids, reconcile, reconciliation_summary,
    render_blind_qa_review_view, render_review_view, render_usage_report, run_generation,
    run_generation_anchors, run_generation_with_approved_anchors, run_mini_labeling,
    run_production_generation, run_production_generation_anchors,
    run_production_generation_with_approved_anchors, split_feasibility_preflight,
    split_reviewed_pool, split_summary, validate_approved_generation_anchors,
    validate_approved_production_generation_anchors, validate_generation_output,
    verify_reviewed_pool_split, write_freeze_manifest, write_generation_anchor_sidecar,
    write_jsonl, write_split_summary,
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
        Some("split") => run_split_cli(&args[1..]),
        Some("gatekeep") => run_gatekeep_cli(&args[1..]),
        Some("freeze") => run_freeze_cli(&args[1..]),
        Some("methodology") => run_methodology_cli(&args[1..]),
        Some("census") => crate::run_census_cli(&args[1..]),
        _ => Err(usage().to_string()),
    }
}

fn usage() -> &'static str {
    "usage:\n  qsf_semantic_datagen generate [--production] [--live] <roster.json> <generation-output.jsonl>\n  qsf_semantic_datagen generate [--production] --anchors-only [--live] <roster.json> <anchors-output.jsonl>\n  qsf_semantic_datagen generate [--production] [--live] --anchors <approved-anchors.jsonl> <roster.json> <generation-output.jsonl>\n  qsf_semantic_datagen label [--live] <generation-output.jsonl> <roster.json> <output-dir> [labeling-run-id]\n  qsf_semantic_datagen reconcile <roster.json> <label-mini.jsonl> <label-fable.jsonl> <reconciliation.jsonl>\n  qsf_semantic_datagen validate-labels <roster.json> <label-interchange.jsonl>\n  qsf_semantic_datagen review [--blind-qa] <roster.json> <labeling-input.jsonl> <label-mini.jsonl> <label-fable.jsonl> <review-decisions.jsonl>\n  qsf_semantic_datagen split <reviewed-pool.jsonl> <seed> <output-dir>\n  qsf_semantic_datagen census <support|rubric-sensitivity|sample-contested|sample-strict|recensus> <roster.json> <pool-id> <date> [sample-seed | <recensus-input.json> <min-relevant-support>] [--lineage-root <path>]\n  qsf_semantic_datagen gatekeep <roster.json> <validation.dataset.jsonl> <test.dataset.jsonl> <blind-qa-decisions.jsonl>\n  qsf_semantic_datagen freeze [--seed <seed>] <roster.json> <dataset-version> <frozen-at> <validation.dataset.jsonl> <test.dataset.jsonl> <generation-output.jsonl> <label-mini.jsonl> <label-fable.jsonl> <reconciliation.jsonl> <review-decisions.jsonl> <blind-qa-decisions.jsonl> <output-dir>\n  qsf_semantic_datagen methodology <freeze-manifest.json> <reconciliation.jsonl> <blind-qa-decisions.jsonl> <validation.dataset.jsonl> <test.dataset.jsonl> <output.md>"
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
    let options = parse_generate_options(args)?;
    if options.positionals.len() != 2 {
        return Err(usage().to_string());
    }
    let roster = RosterSnapshot::from_json_path(options.positionals[0])?;
    let run_id = if options.live {
        "goalrel-generation-live"
    } else {
        "goalrel-generation-replay"
    };
    let transport = selected_transport(options.live, FixtureResponse::Generation)?;
    if transport.kind() == TransportKind::Live {
        engine_logging::engine_info!(
            "goal relevance generation live transport selected run_id={} routine_model_id={} semantic_model_id={}",
            run_id,
            GENERATOR_MODEL_ID,
            SEMANTIC_GENERATOR_MODEL_ID
        );
    }
    if options.anchors_only {
        let run = if options.production {
            run_production_generation_anchors(&transport, &roster, run_id)?
        } else {
            run_generation_anchors(&transport, &roster, run_id)?
        };
        let output_path = Path::new(&options.positionals[1]);
        write_file(
            output_path,
            &write_generation_anchor_sidecar(&run.cluster_anchors)?,
        )?;
        println!(
            "wrote {} cluster anchors to {} using {} transport",
            run.cluster_anchors.len(),
            output_path.display(),
            transport_name(transport.kind())
        );
        render_generation_usage(run.usage_by_model);
        return Ok(());
    }
    let approved_anchor_content = options.anchors_path.as_ref().map(read_file).transpose()?;
    let approved_anchors = approved_anchor_content
        .as_deref()
        .map(parse_generation_anchor_sidecar)
        .transpose()?;
    if let Some(anchors) = &approved_anchors {
        if options.production {
            validate_approved_production_generation_anchors(anchors)?;
        } else {
            validate_approved_generation_anchors(anchors)?;
        }
    }
    let run = match approved_anchors.as_deref() {
        Some(anchors) if options.production => run_production_generation_with_approved_anchors(
            &transport,
            &roster,
            run_id,
            Some(anchors),
        )?,
        None if options.production => run_production_generation(&transport, &roster, run_id)?,
        Some(anchors) => {
            run_generation_with_approved_anchors(&transport, &roster, run_id, Some(anchors))?
        }
        None => run_generation(&transport, &roster, run_id)?,
    };
    validate_generation_output(&run.records)?;
    let feasibility = split_feasibility_preflight(&run.records, 20260721)?;
    let output_path = Path::new(&options.positionals[1]);
    write_file(output_path, &write_jsonl(&run.records)?)?;
    let anchors_path = generation_anchor_sidecar_path(output_path);
    let sidecar_content = approved_anchor_content
        .as_deref()
        .map(ToOwned::to_owned)
        .unwrap_or(write_generation_anchor_sidecar(&run.cluster_anchors)?);
    write_file(&anchors_path, &sidecar_content)?;
    println!(
        "wrote {} generated utterances across {} split components to {} using {} transport",
        run.records.len(),
        feasibility.assignment_by_component.len(),
        output_path.display(),
        transport_name(transport.kind())
    );
    println!(
        "wrote {} cluster anchors to {}",
        run.cluster_anchors.len(),
        anchors_path.display()
    );
    for cluster in &run.cluster_anchors {
        println!(
            "paraphrase cluster {} anchor: {}",
            cluster.cluster_id, cluster.anchor
        );
    }
    render_generation_usage(run.usage_by_model);
    Ok(())
}

struct GenerateOptions<'a> {
    live: bool,
    production: bool,
    anchors_only: bool,
    anchors_path: Option<&'a String>,
    positionals: Vec<&'a String>,
}

fn parse_generate_options(args: &[String]) -> Result<GenerateOptions<'_>, String> {
    let mut live = false;
    let mut production = false;
    let mut anchors_only = false;
    let mut anchors_path = None;
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--live" if !live => live = true,
            "--production" if !production => production = true,
            "--anchors-only" if !anchors_only => anchors_only = true,
            "--anchors" if anchors_path.is_none() => {
                index += 1;
                let path = args.get(index).ok_or_else(|| usage().to_string())?;
                anchors_path = Some(path);
            }
            value if value.starts_with("--") => return Err(usage().to_string()),
            _ => positionals.push(&args[index]),
        }
        index += 1;
    }
    if anchors_only && anchors_path.is_some() {
        return Err("generate --anchors-only cannot also accept --anchors".to_string());
    }
    Ok(GenerateOptions {
        live,
        production,
        anchors_only,
        anchors_path,
        positionals,
    })
}

fn render_generation_usage(usage_by_model: Option<std::collections::BTreeMap<String, TokenUsage>>) {
    match usage_by_model {
        Some(usage_by_model) => {
            for (model_id, usage) in usage_by_model {
                println!("{}", render_usage_report(&model_id, usage));
            }
        }
        None => println!("generation token usage unavailable from replay transport"),
    }
}

pub(crate) fn generation_anchor_sidecar_path(output_path: &Path) -> PathBuf {
    output_path.with_file_name("generation-anchors.jsonl")
}

fn run_split_cli(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err(usage().to_string());
    }
    let reviewed = parse_reviewed_pool(&read_file(&args[0])?)?;
    let seed = args[1]
        .parse::<u64>()
        .map_err(|error| format!("split seed must be an unsigned integer: {error}"))?;
    let split = split_reviewed_pool(&reviewed, seed)?;
    let output = Path::new(&args[2]);
    fs::create_dir_all(output).map_err(|error| {
        format!(
            "could not create split output directory {}: {error}",
            output.display()
        )
    })?;
    write_file(
        output.join("validation.dataset.jsonl"),
        &write_jsonl(&split.validation)?,
    )?;
    write_file(
        output.join("test.dataset.jsonl"),
        &write_jsonl(&split.test)?,
    )?;
    write_file(
        output.join("split-summary.json"),
        &write_split_summary(&split_summary(&split, seed))?,
    )?;
    println!(
        "wrote deterministic validation/test split with seed {seed} across {} components",
        split.assignment_by_component.len()
    );
    Ok(())
}

fn run_gatekeep_cli(args: &[String]) -> Result<(), String> {
    if args.len() != 4 {
        return Err(usage().to_string());
    }
    let roster = RosterSnapshot::from_json_path(&args[0])?;
    let validation = parse_reviewed_pool(&read_file(&args[1])?)?;
    let test = parse_reviewed_pool(&read_file(&args[2])?)?;
    let blind_qa = parse_review_decisions(&read_file(&args[3])?)?;
    gatekeep_goal_relevance_freeze(GatekeeperRequest {
        validation: &validation,
        test: &test,
        roster: &roster,
        blind_qa_decisions: &blind_qa,
    })?;
    println!("goal relevance freeze gatekeeper passed");
    Ok(())
}

fn run_freeze_cli(args: &[String]) -> Result<(), String> {
    let options = parse_freeze_options(args)?;
    let args = options.positionals;
    if args.len() != 12 {
        return Err(usage().to_string());
    }
    let roster = RosterSnapshot::from_json_path(args[0])?;
    let validation_text = read_file(args[3])?;
    let test_text = read_file(args[4])?;
    let validation = parse_reviewed_pool(&validation_text)?;
    let test = parse_reviewed_pool(&test_text)?;
    let seed = match options.seed_override {
        Some(seed) => seed,
        None => {
            let summary_path = split_summary_path(Path::new(&args[3]));
            parse_split_summary(&read_file(&summary_path)?)?.split_seed
        }
    };
    let generation = read_file(args[5])?;
    let mini = read_file(args[6])?;
    let fable = read_file(args[7])?;
    let reconciliation_text = read_file(args[8])?;
    let review_decisions = read_file(args[9])?;
    let blind_qa_text = read_file(args[10])?;
    let blind_qa = parse_review_decisions(&blind_qa_text)?;
    gatekeep_goal_relevance_freeze(GatekeeperRequest {
        validation: &validation,
        test: &test,
        roster: &roster,
        blind_qa_decisions: &blind_qa,
    })?;
    if let Err(error) = verify_reviewed_pool_split(
        args[1],
        seed,
        &validation,
        &test,
        &validation_text,
        &test_text,
    ) {
        engine_logging::engine_error!(
            "goal relevance freeze split reproduction failed operation=freeze dataset_version={} seed={} error={}",
            args[1],
            seed,
            error
        );
        return Err(error);
    }
    let artifacts = freeze_artifacts(FreezeRequest {
        dataset_version: args[1],
        roster: &roster,
        split_seed: seed,
        frozen_at: args[2],
        validation: &validation,
        test: &test,
        generation_output_sha256: &crate::sha256(&generation),
        label_mini_sha256: &crate::sha256(&mini),
        label_fable_sha256: &crate::sha256(&fable),
        review_decisions_sha256: &crate::sha256(&review_decisions),
    })?;
    let output = Path::new(&args[11]);
    let lineage = output.join("lineage").join(args[1]);
    fs::create_dir_all(&lineage).map_err(|error| {
        format!(
            "could not create lineage directory {}: {error}",
            lineage.display()
        )
    })?;
    write_file(
        output.join("validation.dataset.jsonl"),
        &artifacts.validation_jsonl,
    )?;
    write_file(output.join("test.dataset.jsonl"), &artifacts.test_jsonl)?;
    write_file(
        output.join("freeze-manifest.json"),
        &write_freeze_manifest(&artifacts.manifest)?,
    )?;
    write_file(lineage.join("generation-output.jsonl"), &generation)?;
    let labeling_input = build_labeling_input(&parse_generation_output(&generation)?, &roster)?;
    write_file(
        lineage.join("labeling-input.jsonl"),
        &write_jsonl(&labeling_input)?,
    )?;
    write_file(lineage.join("label-mini.jsonl"), &mini)?;
    write_file(lineage.join("label-fable.jsonl"), &fable)?;
    write_file(lineage.join("reconciliation.jsonl"), &reconciliation_text)?;
    write_file(lineage.join("review-decisions.jsonl"), &review_decisions)?;
    write_file(lineage.join("blind-qa-decisions.jsonl"), &blind_qa_text)?;
    let reviewed = crate::canonical_reviewed_pool(&validation, &test);
    write_file(
        lineage.join("reviewed-pool.jsonl"),
        &write_jsonl(&reviewed)?,
    )?;
    let reconciliation = parse_reconciliation(&reconciliation_text)?;
    let all = validation.iter().chain(test.iter()).collect::<Vec<_>>();
    let qa = crate::blind_qa_agreement_by_slice(&all, &blind_qa)?;
    write_file(
        lineage.join("reconciliation-summary.json"),
        &serde_json::to_string_pretty(&reconciliation_summary(&reconciliation))
            .map_err(|error| error.to_string())?,
    )?;
    write_file(
        output.join("DatasetMethodology.GoalRelevance.md"),
        &methodology_goal_relevance(
            &artifacts.manifest,
            &reconciliation_summary(&reconciliation),
            &qa,
        ),
    )?;
    println!(
        "froze goal relevance dataset {} under {}",
        args[1],
        output.display()
    );
    Ok(())
}

struct FreezeOptions<'a> {
    seed_override: Option<u64>,
    positionals: Vec<&'a String>,
}

fn parse_freeze_options(args: &[String]) -> Result<FreezeOptions<'_>, String> {
    let mut seed_override = None;
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--seed" if seed_override.is_none() => {
                index += 1;
                let value = args.get(index).ok_or_else(|| usage().to_string())?;
                seed_override =
                    Some(value.parse::<u64>().map_err(|error| {
                        format!("split seed must be an unsigned integer: {error}")
                    })?);
            }
            value if value.starts_with("--") => return Err(usage().to_string()),
            _ => positionals.push(&args[index]),
        }
        index += 1;
    }
    Ok(FreezeOptions {
        seed_override,
        positionals,
    })
}

pub(crate) fn split_summary_path(validation_path: &Path) -> PathBuf {
    validation_path.with_file_name("split-summary.json")
}

fn run_methodology_cli(args: &[String]) -> Result<(), String> {
    if args.len() != 6 {
        return Err(usage().to_string());
    }
    let manifest = parse_freeze_manifest(&read_file(&args[0])?)?;
    let reconciliation = reconciliation_summary(&parse_reconciliation(&read_file(&args[1])?)?);
    let blind_qa = parse_review_decisions(&read_file(&args[2])?)?;
    let validation = parse_reviewed_pool(&read_file(&args[3])?)?;
    let test = parse_reviewed_pool(&read_file(&args[4])?)?;
    let all = validation.iter().chain(test.iter()).collect::<Vec<_>>();
    let qa: std::collections::BTreeMap<String, AgreementEvidence> =
        crate::blind_qa_agreement_by_slice(&all, &blind_qa)?;
    write_file(
        &args[5],
        &methodology_goal_relevance(&manifest, &reconciliation, &qa),
    )
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
    let summary = reconciliation_summary(&reconciliation);
    let summary_path = Path::new(&args[3]).with_file_name("reconciliation-summary.json");
    write_file(
        &summary_path,
        &serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?,
    )?;
    println!(
        "wrote {} reconciliation pairs and {} ; mini/Fable agreement {}/{} ({:.2}%)",
        reconciliation.len(),
        summary_path.display(),
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
