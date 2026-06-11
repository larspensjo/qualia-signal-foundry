use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use clap::Parser;

pub const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
pub const DEFAULT_PORT: u16 = 3940;
pub const DEFAULT_STATE_DIR: &str = "state/realtime";

#[derive(Clone, Debug, Parser)]
#[command(
    name = "qsf_realtime_server",
    about = "Realtime browser voice conversation server"
)]
pub struct Args {
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

impl Args {
    pub fn parse_from_env() -> Self {
        Self::parse()
    }
}
