use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
pub const DEFAULT_PORT: u16 = 3940;
pub const DEFAULT_STATE_DIR: &str = "state/realtime";
pub const DEFAULT_PROBE_PHRASE_SET: &str = "smoke";
pub const DEFAULT_TURN_DELAY_MS: u64 = 250;
pub const DEFAULT_TURN_TIMEOUT_MS: u64 = 120_000;
pub const DEFAULT_ATTACH_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_FORMATION_TIMEOUT_MS: u64 = 60_000;

#[derive(Clone, Debug, Parser)]
#[command(
    name = "qsf_realtime_server",
    about = "Realtime browser voice conversation server"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long, default_value = DEFAULT_STATE_DIR)]
    pub state_dir: PathBuf,

    #[arg(long, default_value_t = DEFAULT_HOST)]
    pub host: IpAddr,

    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Allocate a fresh UUID-backed QSF session id for each browser session.
    /// By default the realtime server uses a stable `default` session id so
    /// local memory and continuity artifacts are reusable across runs.
    #[arg(long)]
    pub random_session_id: bool,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Run a scripted realtime conversation without an HTTP listener.
    Probe(ProbeArgs),
}

#[derive(Clone, Debug, clap::Args)]
pub struct ProbeArgs {
    #[arg(long, default_value = DEFAULT_PROBE_PHRASE_SET)]
    pub phrase_set: String,
    /// Run directory, used exactly as given. It must not contain an earlier probe ledger.
    #[arg(long)]
    pub state_dir: Option<PathBuf>,
    #[arg(long)]
    pub run_id: Option<String>,
    #[arg(long)]
    pub cold_start: bool,
    #[arg(long, default_value_t = DEFAULT_TURN_DELAY_MS)]
    pub turn_delay_ms: u64,
    #[arg(long, default_value_t = DEFAULT_TURN_TIMEOUT_MS)]
    pub turn_timeout_ms: u64,
    #[arg(long, default_value_t = DEFAULT_ATTACH_TIMEOUT_MS)]
    pub attach_timeout_ms: u64,
    #[arg(long, default_value_t = DEFAULT_FORMATION_TIMEOUT_MS)]
    pub formation_timeout_ms: u64,
    #[arg(long)]
    pub git_commit: Option<String>,
    #[arg(long, value_name = "DIR")]
    pub seed_only: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub structure_only: Option<PathBuf>,
}

impl Args {
    pub fn parse_from_env() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::{Args, Command, DEFAULT_PROBE_PHRASE_SET, DEFAULT_TURN_DELAY_MS};
    use clap::Parser;

    #[test]
    fn no_subcommand_keeps_browser_server_defaults() {
        let args = Args::try_parse_from(["qsf_realtime_server"]).expect("parse");
        assert!(args.command.is_none());
    }

    #[test]
    fn probe_defaults_and_auxiliary_modes_parse() {
        let args = Args::try_parse_from(["qsf_realtime_server", "probe"]).expect("parse");
        let Command::Probe(probe) = args.command.expect("probe command");
        assert_eq!(probe.phrase_set, DEFAULT_PROBE_PHRASE_SET);
        assert_eq!(probe.turn_delay_ms, DEFAULT_TURN_DELAY_MS);
        let seed = Args::try_parse_from(["qsf_realtime_server", "probe", "--seed-only", "x"])
            .expect("seed parse");
        let Command::Probe(seed) = seed.command.expect("seed command");
        assert_eq!(seed.seed_only.expect("seed path").to_string_lossy(), "x");
        let structure =
            Args::try_parse_from(["qsf_realtime_server", "probe", "--structure-only", "x"])
                .expect("structure parse");
        let Command::Probe(structure) = structure.command.expect("structure command");
        assert_eq!(
            structure
                .structure_only
                .expect("structure path")
                .to_string_lossy(),
            "x"
        );
    }
}
