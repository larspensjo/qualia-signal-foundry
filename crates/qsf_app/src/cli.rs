use std::path::PathBuf;

use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};

use crate::experiments::{self, ExperimentName};
use crate::world_corpus::{DEFAULT_WORLD_CORPUS_LEDGER_PATH, WorldCorpusIngestOptions};

#[derive(Debug, Parser)]
#[command(name = "qsf_app")]
#[command(about = "Qualia Signal Foundry experiment runner")]
#[command(version)]
struct Cli {
    /// Disable ANSI color in interactive console output.
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a named experiment.
    Experiment {
        /// Experiment id, such as framework-skeleton-mvp.
        name: ExperimentName,

        /// Repository/workspace root used by experiments that need repo-relative resources.
        #[arg(long, value_name = "PATH")]
        workspace_root: Option<PathBuf>,
    },

    /// Run a first-class sleep/consolidation update over persisted realtime state.
    Sleep {
        /// State directory to sleep; defaults to the realtime continuity state.
        #[arg(long, default_value = "state/realtime", value_name = "PATH")]
        state_dir: PathBuf,

        /// Model provider to use for summarization and sleep maintenance.
        #[arg(long, value_name = "PROVIDER")]
        provider: Option<String>,

        /// Repository/workspace root used by sleep maintenance that needs repo-relative resources.
        #[arg(long, value_name = "PATH")]
        workspace_root: Option<PathBuf>,

        /// Producer-owned corpus output directory for offline world-memory consolidation.
        #[arg(long, value_name = "PATH")]
        corpus_path: Option<PathBuf>,

        /// Shared content-hash corpus ledger used to derive the sleep delta.
        #[arg(long, default_value = DEFAULT_WORLD_CORPUS_LEDGER_PATH, value_name = "PATH")]
        ledger_path: PathBuf,
    },

    /// Print full persisted volition goal definitions and lifecycle state.
    Goals {
        /// Realtime state directory containing continuity sessions.
        #[arg(long, default_value = "state/realtime", value_name = "PATH")]
        state_dir: PathBuf,

        /// Explicit continuity session id. Bypasses automatic session selection.
        #[arg(long, value_name = "ID")]
        session: Option<String>,

        /// Render the existing human-readable console view instead of JSONL.
        #[arg(long)]
        pretty: bool,

        /// Write the rendered report to this file instead of stdout.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },

    /// Print a realtime session's turns joined to their volition traces, as JSONL.
    Transcript {
        /// Realtime state directory containing the diagnostics ledger.
        #[arg(long, default_value = "state/realtime", value_name = "PATH")]
        state_dir: PathBuf,

        /// Explicit session id. Bypasses automatic ledger selection.
        #[arg(long, value_name = "ID")]
        session: Option<String>,

        /// Emit every run in the ledger, not just the most recent one.
        #[arg(long)]
        all: bool,

        /// Indent each record. Readable, but no longer one record per line.
        #[arg(long)]
        pretty: bool,

        /// Attach the verbatim traces and the full exchange to each turn.
        #[arg(long)]
        full: bool,

        /// Write to this file instead of stdout.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },

    /// Build or incrementally refresh the read-only external world corpus index.
    IngestWorld {
        /// Producer-owned corpus output directory. When omitted, QSF_WORLD_CORPUS_PATH is used
        /// and QSF falls back to its bundled fixture if that path is unset or unavailable.
        #[arg(long, value_name = "PATH")]
        corpus_path: Option<PathBuf>,

        /// Persistent content-hash ledger and index artifact location.
        #[arg(
            long,
            default_value = DEFAULT_WORLD_CORPUS_LEDGER_PATH,
            value_name = "PATH"
        )]
        ledger_path: PathBuf,
    },

    /// List experiments available in this build.
    ListExperiments,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    crate::console::styling::set_no_color_flag(cli.no_color);

    match cli.command {
        Some(Command::Experiment {
            name,
            workspace_root,
        }) => {
            let summary = experiments::run_experiment_with_workspace_root(name, workspace_root)?;
            println!(
                "Experiment `{}` completed. Run artifacts: {}",
                summary.experiment_id,
                summary.run_dir.display()
            );
            Ok(())
        }
        Some(Command::Sleep {
            state_dir,
            provider,
            workspace_root,
            corpus_path,
            ledger_path,
        }) => {
            let requested_provider =
                provider.unwrap_or_else(|| qsf_models::requested_provider_from_env().to_string());
            let summary = crate::sleep::run_sleep_update(crate::sleep::SleepUpdateOptions {
                state_dir,
                requested_provider,
                workspace_root,
                world_corpus_path: corpus_path,
                world_corpus_ledger_path: ledger_path,
            })?;
            println!(
                "{}",
                crate::sleep::render_change_view(&summary.change_record)
            );
            println!(
                "Sleep update completed. State: {}. Run artifacts: {}",
                summary.state_dir.display(),
                summary.run_dir.display()
            );
            Ok(())
        }
        Some(Command::Goals {
            state_dir,
            session,
            pretty,
            out,
        }) => {
            let loaded = crate::goal_detail_loading::load_goal_detail_report(
                &state_dir,
                session.as_deref(),
            )?;
            let rendered = crate::goal_report::render_goal_report(
                &loaded.report,
                crate::console::goal_detail_view::GoalDetailHeader {
                    session_id: &loaded.session_id,
                    seed_fixture_id: &loaded.seed_fixture_id,
                    recorded_at: &loaded.recorded_at,
                },
                pretty,
            )?;
            match out {
                Some(destination) => {
                    std::fs::write(&destination, rendered).with_context(|| {
                        format!(
                            "failed to write goals report to `{}`",
                            destination.display()
                        )
                    })?;
                    eprintln!("wrote {}", destination.display());
                }
                None if pretty => println!("{rendered}"),
                None => print!("{rendered}"),
            }
            Ok(())
        }
        Some(Command::Transcript {
            state_dir,
            session,
            all,
            pretty,
            full,
            out,
        }) => {
            let path = crate::transcript::resolve_ledger_path(&state_dir, session.as_deref())?;
            let entries = crate::transcript::load_ledger(&path)?;

            let ledger_label = path.display().to_string();
            let runs = crate::transcript::runs_from_entries(entries, &ledger_label, full);
            let runs = crate::transcript::select_runs(runs, all);

            // Warnings duplicate what `header.source` already carries in the artifact. They exist so
            // an interactive run notices; the artifact does not depend on anyone having read them.
            for warning in crate::transcript::render_source_warnings(&runs) {
                eprintln!("{warning}");
            }

            let rendered = crate::transcript::render_runs(runs, pretty)?;
            match out {
                Some(destination) => {
                    std::fs::write(&destination, rendered).with_context(|| {
                        format!("failed to write transcript to `{}`", destination.display())
                    })?;
                    eprintln!("wrote {}", destination.display());
                }
                None => print!("{rendered}"),
            }
            Ok(())
        }
        Some(Command::IngestWorld {
            corpus_path,
            ledger_path,
        }) => {
            let summary = crate::world_corpus::ingest_world_corpus(WorldCorpusIngestOptions {
                corpus_path,
                ledger_path,
            })?;
            println!("{}", summary.render());
            Ok(())
        }
        Some(Command::ListExperiments) => {
            for experiment in experiments::available_experiments() {
                println!("{}\t{}", experiment.id, experiment.description);
            }

            Ok(())
        }
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::Cli;

    #[test]
    fn cli_help_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_color_flag_is_accepted_before_or_after_subcommand() {
        let before = Cli::try_parse_from(["qsf_app", "--no-color", "list-experiments"]).unwrap();
        let after = Cli::try_parse_from(["qsf_app", "list-experiments", "--no-color"]).unwrap();

        assert!(before.no_color);
        assert!(after.no_color);
    }

    #[test]
    fn sleep_command_defaults_to_realtime_state() {
        let cli = Cli::try_parse_from(["qsf_app", "sleep"]).unwrap();

        let Some(super::Command::Sleep { state_dir, .. }) = cli.command else {
            panic!("expected sleep command");
        };
        assert_eq!(state_dir, std::path::PathBuf::from("state/realtime"));
    }

    #[test]
    fn goals_command_defaults_to_realtime_state_and_parses_session() {
        let default_cli = Cli::try_parse_from(["qsf_app", "goals"]).unwrap();
        let Some(super::Command::Goals {
            state_dir,
            session,
            pretty,
            out,
        }) = default_cli.command
        else {
            panic!("expected goals command");
        };
        assert_eq!(state_dir, std::path::PathBuf::from("state/realtime"));
        assert_eq!(session, None);
        assert!(!pretty);
        assert_eq!(out, None);

        let session_cli =
            Cli::try_parse_from(["qsf_app", "goals", "--session", "run-123"]).unwrap();
        let Some(super::Command::Goals { session, .. }) = session_cli.command else {
            panic!("expected goals command");
        };
        assert_eq!(session.as_deref(), Some("run-123"));
    }

    #[test]
    fn goals_command_parses_pretty_and_out() {
        let cli =
            Cli::try_parse_from(["qsf_app", "goals", "--pretty", "--out", "goals.jsonl"]).unwrap();

        let Some(super::Command::Goals { pretty, out, .. }) = cli.command else {
            panic!("expected goals command");
        };
        assert!(pretty);
        assert_eq!(out, Some(std::path::PathBuf::from("goals.jsonl")));
    }

    #[test]
    fn transcript_command_defaults_to_realtime_state_and_compact_output() {
        let cli = Cli::try_parse_from(["qsf_app", "transcript"]).unwrap();

        let Some(super::Command::Transcript {
            state_dir,
            session,
            all,
            pretty,
            full,
            out,
        }) = cli.command
        else {
            panic!("expected transcript command");
        };
        assert_eq!(state_dir, std::path::PathBuf::from("state/realtime"));
        assert_eq!(session, None);
        assert!(!all);
        assert!(!pretty);
        assert!(!full);
        assert_eq!(out, None);
    }

    #[test]
    fn transcript_command_parses_every_flag() {
        let cli = Cli::try_parse_from([
            "qsf_app",
            "transcript",
            "--session",
            "run-123",
            "--all",
            "--pretty",
            "--full",
            "--out",
            "turns.jsonl",
        ])
        .unwrap();

        let Some(super::Command::Transcript {
            session,
            all,
            pretty,
            full,
            out,
            ..
        }) = cli.command
        else {
            panic!("expected transcript command");
        };
        assert_eq!(session.as_deref(), Some("run-123"));
        assert!(all);
        assert!(pretty);
        assert!(full);
        assert_eq!(out, Some(std::path::PathBuf::from("turns.jsonl")));
    }

    #[test]
    fn no_color_flag_is_accepted_around_goals_subcommand() {
        let before = Cli::try_parse_from(["qsf_app", "--no-color", "goals"]).unwrap();
        let after = Cli::try_parse_from(["qsf_app", "goals", "--no-color"]).unwrap();

        assert!(before.no_color);
        assert!(after.no_color);
    }

    #[test]
    fn world_ingest_command_uses_the_default_ledger_path() {
        let cli = Cli::try_parse_from(["qsf_app", "ingest-world"]).unwrap();

        let Some(super::Command::IngestWorld { ledger_path, .. }) = cli.command else {
            panic!("expected ingest-world command");
        };
        assert_eq!(
            ledger_path,
            std::path::PathBuf::from(crate::world_corpus::DEFAULT_WORLD_CORPUS_LEDGER_PATH)
        );
    }
}
