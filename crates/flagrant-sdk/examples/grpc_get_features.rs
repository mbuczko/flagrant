use flagrant_sdk::{AsyncFlagrantClient, GrpcTransport};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:50061".to_string());
    let project = std::env::args().nth(2).unwrap_or_else(|| "smoke".into());
    let environment = std::env::args().nth(3).unwrap_or_else(|| "dev".into());
    let identity = std::env::args().nth(4).unwrap_or_else(|| "alice".into());

    let transport = GrpcTransport::connect(endpoint, None).await?;
    let client = AsyncFlagrantClient::new(transport, project);
    let features = client.get_features(&environment, &identity).await?;

    println!("{features:#?}");

    Ok(())
}
