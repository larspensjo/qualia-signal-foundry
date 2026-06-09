pub mod cli;
pub mod diagnostics;
pub mod health;
pub mod realtime;
pub mod server;
pub mod state;

pub async fn run() -> anyhow::Result<()> {
    server::serve(cli::Args::parse_from_env()).await
}
