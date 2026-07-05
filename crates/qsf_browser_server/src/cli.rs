use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use clap::Parser;

/// Defaults are chosen so the workbench launches with no arguments against
/// the runtime's standard memory store location.
pub const DEFAULT_STORE_PATH: &str = "state/realtime/continuity/default/memory-store.json";
pub const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
pub const DEFAULT_PORT: u16 = 3939;

#[derive(Clone, Debug, Parser)]
#[command(
    name = "qsf_browser_server",
    about = "Memory Association Browser server"
)]
pub struct Args {
    /// Path to the persisted memory store.
    #[arg(long, default_value = DEFAULT_STORE_PATH)]
    pub store: PathBuf,

    /// Host interface to bind.
    #[arg(long, default_value_t = DEFAULT_HOST)]
    pub host: IpAddr,

    /// TCP port to bind.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
}

impl Args {
    pub fn parse_from_env() -> Self {
        Self::parse()
    }
}
