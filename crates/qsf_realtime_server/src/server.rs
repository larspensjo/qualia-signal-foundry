use std::net::{IpAddr, SocketAddr};

use axum::Router;

use crate::cli::Args;
use crate::health;
use crate::realtime;
use crate::state::AppState;

pub async fn serve(args: Args) -> anyhow::Result<()> {
    engine_logging::initialize();

    if !is_loopback(args.host) {
        anyhow::bail!(
            "refusing to bind qsf_realtime_server to non-loopback host {} because it uses the server-held OPENAI_API_KEY",
            args.host
        );
    }

    let state = AppState::load(&args)?;
    log_startup_summary(&args, &state);

    let app = Router::new()
        .merge(health::router())
        .merge(realtime::router())
        .with_state(state);

    let addr = SocketAddr::new(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("listening on http://{}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Resolves when the process receives a Ctrl+C / console interrupt.
///
/// Awaiting `tokio::signal::ctrl_c` installs a handler that reports the event as
/// handled, which keeps Windows from terminating the process through its default
/// handler with `STATUS_CONTROL_C_EXIT` (0xC000013A). Without it `cargo run` sees
/// that exit code and reports a spurious failure when the dev server is stopped
/// with Ctrl+C, even though stopping that way is the intended way to halt it.
async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        log::error!("failed to listen for Ctrl+C shutdown signal: {err}");
    }
}

fn log_startup_summary(args: &Args, state: &AppState) {
    log::info!("state dir: {}", state.state_dir().display());
    log::info!("diagnostics dir: {}", state.diagnostics_dir().display());
    log::info!("OpenAI base URL: {}", state.openai_base_url());
    if !is_loopback(args.host) {
        log::warn!("non-loopback bind requested for qsf_realtime_server");
    }
}

fn is_loopback(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
