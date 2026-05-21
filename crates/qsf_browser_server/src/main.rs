use qsf_browser_server::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}
