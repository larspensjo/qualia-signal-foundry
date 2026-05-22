//! HTTP server for post-hoc inspection of QSF persisted artifacts.
//!
//! Read-only. Depends on `qsf_memory`, never on `qsf_app`.

pub mod cli;
pub mod health;
pub mod memory;
pub mod server;
pub mod session_context;
pub mod state;
pub mod web;

pub async fn run() -> anyhow::Result<()> {
    let args = cli::Args::parse_from_env();
    server::serve(args).await
}
