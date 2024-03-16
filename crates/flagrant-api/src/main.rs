use flagrant_api::{init_tracing, start_api_server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    start_api_server().await.expect("Cannot start API server");

    Ok(())
}
