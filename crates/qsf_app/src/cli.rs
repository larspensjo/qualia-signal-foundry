use clap::{CommandFactory, Parser, Subcommand};

use crate::experiments::{self, ExperimentName};

#[derive(Debug, Parser)]
#[command(name = "qsf_app")]
#[command(about = "Qualia Signal Foundry experiment runner")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a named experiment.
    Experiment {
        /// Experiment id, such as framework-skeleton-mvp.
        name: ExperimentName,
    },

    /// List experiments available in this build.
    ListExperiments,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Experiment { name }) => experiments::run_placeholder(name),
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
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn cli_help_is_valid() {
        Cli::command().debug_assert();
    }
}
