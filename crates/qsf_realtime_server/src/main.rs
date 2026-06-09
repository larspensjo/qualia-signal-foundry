#[tokio::main]
async fn main() -> anyhow::Result<()> {
    qsf_realtime_server::run().await
}
