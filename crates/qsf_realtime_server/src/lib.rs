pub mod cli;
pub mod diagnostics;
pub mod health;
pub mod realtime;
pub mod server;
pub mod state;

pub use realtime::RAW_OUTPUT_AUDIO_DELTA_EVENT_TYPES;
pub use realtime::injection::DEFAULT_PCM_RATE_HZ;
pub use realtime::sideband_attachment::SidebandAttachment;
pub use realtime::sideband_connection::format_connect_error;

pub const OPENAI_SAFETY_IDENTIFIER_HEADER: &str =
    realtime::safety_identifier::OPENAI_SAFETY_IDENTIFIER_HEADER;

pub fn hash_session_id(session_id: &str) -> String {
    realtime::safety_identifier::hash_session_id(session_id)
}

pub async fn run() -> anyhow::Result<()> {
    server::serve(cli::Args::parse_from_env()).await
}
