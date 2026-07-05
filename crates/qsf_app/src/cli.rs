use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

use crate::experiments::{self, ExperimentName};

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
        }) => {
            let requested_provider =
                provider.unwrap_or_else(|| qsf_models::requested_provider_from_env().to_string());
            let summary = crate::sleep::run_sleep_update(crate::sleep::SleepUpdateOptions {
                state_dir,
                requested_provider,
                workspace_root,
            })?;
            println!(
                "Sleep update completed. State: {}. Run artifacts: {}",
                summary.state_dir.display(),
                summary.run_dir.display()
            );
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
}
