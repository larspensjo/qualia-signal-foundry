use std::net::{IpAddr, SocketAddr};

use axum::Router;

use crate::cli::Args;
use crate::health;
use crate::memory::routes;
use crate::session_context;
use crate::state::AppState;
use crate::web;

pub async fn serve(args: Args) -> anyhow::Result<()> {
    // `engine_logging::initialize()` wires stderr and ./engine.log logging.
    engine_logging::initialize();

    let state = AppState::load(&args);
    log_startup_summary(&args, &state);

    let app = Router::new()
        .merge(web::router())
        .merge(health::router())
        .merge(routes::router())
        .merge(session_context::router())
        .with_state(state);

    let addr = SocketAddr::new(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn log_startup_summary(args: &Args, state: &AppState) {
    log::info!("memory store path: {}", state.store_path().display());
    log::info!(
        "session state path: {}",
        state.session_state_path().display()
    );
    match state.loaded() {
        Ok(loaded) => log::info!(
            "store loaded: {} records, {} associations",
            loaded.contents.records.len(),
            loaded.contents.associations.len()
        ),
        Err(err) => log::warn!("store failed to load: {err}"),
    }
    if !is_loopback(args.host) {
        log::warn!(
            "binding to {} (non-loopback). The Memory Association Browser serves memory contents over HTTP; this address may be reachable from other hosts.",
            args.host
        );
    }
}

fn is_loopback(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
