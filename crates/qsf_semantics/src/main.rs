use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use qsf_semantics::bench::{
    BenchOptions, DEFAULT_CANDIDATE_COUNTS_CSV, DEFAULT_CONVERSATION_TURNS,
    DEFAULT_JUDGE_WORKLOADS_PER_TURN, DEFAULT_MAX_TOTAL_REQUESTS,
    DEFAULT_PER_CANDIDATE_REPETITIONS, DEFAULT_REPETITIONS, DEFAULT_TARGET_TURNS_PER_MINUTE,
    default_run_id, run_directory, run_from_env,
};

#[derive(Debug, Parser)]
#[command(
    name = "qsf_semantics",
    about = "Semantic scoring and measurement tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Measure pair-scoring latency, request shaping, and usage-based cost.
    Bench(BenchArgs),
}

#[derive(Debug, clap::Args)]
struct BenchArgs {
    /// Candidate counts to measure, comma-separated.
    #[arg(long, value_delimiter = ',', default_value = DEFAULT_CANDIDATE_COUNTS_CSV)]
    candidate_counts: Vec<usize>,
    /// Repeated turns per feasible cell.
    #[arg(long, default_value_t = DEFAULT_REPETITIONS)]
    repetitions: usize,
    /// Repeated turns for the smallest per-candidate cell.
    #[arg(long, default_value_t = DEFAULT_PER_CANDIDATE_REPETITIONS)]
    per_candidate_repetitions: usize,
    /// Maximum reserved HTTP requests for the whole run.
    #[arg(long, default_value_t = DEFAULT_MAX_TOTAL_REQUESTS)]
    max_total_requests: u64,
    /// Sustained turns per minute required for per-candidate request feasibility.
    #[arg(long, default_value_t = DEFAULT_TARGET_TURNS_PER_MINUTE)]
    target_turns_per_minute: u64,
    /// Judge workloads sharing RPM per spoken turn.
    #[arg(long, default_value_t = DEFAULT_JUDGE_WORKLOADS_PER_TURN)]
    judge_workloads_per_turn: u64,
    /// Turns assumed when calculating conversation cost.
    #[arg(long, default_value_t = DEFAULT_CONVERSATION_TURNS)]
    conversation_turns: u64,
    /// Non-judge candidate/context/send overhead supplied by the operator, in ms.
    #[arg(long, default_value_t = 0)]
    local_overhead_ms: u64,
    /// Operator description of the network used during measurement.
    #[arg(long, default_value = "not separately characterized by operator")]
    network_description: String,
    /// Root directory for run artifacts.
    #[arg(long, default_value = "runs")]
    output_root: PathBuf,
    /// Optional run id; a timestamped id is generated when omitted.
    #[arg(long)]
    run_id: Option<String>,
    /// Print and persist the plan without sending requests.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

impl From<BenchArgs> for BenchOptions {
    fn from(args: BenchArgs) -> Self {
        let run_id = args.run_id.unwrap_or_else(default_run_id);
        let run_directory = run_directory(&args.output_root, &run_id);
        BenchOptions {
            candidate_counts: args.candidate_counts,
            repetitions: args.repetitions,
            per_candidate_repetitions: args.per_candidate_repetitions,
            max_total_requests: args.max_total_requests,
            target_turns_per_minute: args.target_turns_per_minute,
            judge_workloads_per_turn: args.judge_workloads_per_turn,
            conversation_turns: args.conversation_turns,
            local_overhead_ms: args.local_overhead_ms,
            network_description: args.network_description,
            run_directory,
            run_id,
            dry_run: args.dry_run,
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    engine_logging::initialize();
    let cli = Cli::parse();
    match cli.command {
        Command::Bench(args) => match run_from_env(args.into()).await {
            Ok(outcome) => {
                println!("Bench outcome: {outcome:?}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("bench failed: {error}");
                ExitCode::FAILURE
            }
        },
    }
}
